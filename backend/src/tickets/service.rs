//! Aturan tiket packing: semua perubahan satu transaksi database diawali `repo::kunci()` karena status tanpa kunci bisa basi dan serah-terima (mengurangi stok) paling sering ditekan dua kali.

use super::repo::{self, TiketTerkunci};
use super::TicketStatus;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Membuat tiket pesanan marketplace; barang yang listing-nya belum dipetakan membuatnya DITOLAK agar paket tak kurang, dan Owner diberi tahu barang yang perlu dipetakan.
pub async fn buat(pool: &PgPool, external_order_id: Uuid, notes: Option<&str>) -> AppResult<Uuid> {
    let mut tx = pool.begin().await?;

    if !repo::ada_pesanan(&mut tx, external_order_id).await? {
        return Err(AppError::not_found("Pesanan tidak ditemukan."));
    }

    let baris = repo::baris_pesanan(&mut tx, external_order_id).await?;
    if baris.is_empty() {
        return Err(AppError::conflict(
            "Pesanan ini belum punya rincian barang, jadi tiket packing belum bisa dibuat.",
        ));
    }

    let belum_dipetakan: Vec<&str> = baris
        .iter()
        .filter(|b| b.product_id.is_none())
        .map(|b| b.item_name_snapshot.as_str())
        .collect();

    if !belum_dipetakan.is_empty() {
        return Err(AppError::conflict(format!(
            "Barang berikut belum dipetakan ke produk: {}. Petakan dulu di halaman produk.",
            belum_dipetakan.join(", ")
        )));
    }

    // Dua listing bisa menunjuk produk sama; digabung agar pengepak melihat satu baris "ambil 3".
    let nama: HashMap<Uuid, String> = baris
        .iter()
        .filter_map(|b| {
            let id = b.product_id?;
            Some((
                id,
                b.product_name
                    .clone()
                    .unwrap_or_else(|| b.item_name_snapshot.clone()),
            ))
        })
        .collect();

    let lines = stock::gabungkan_baris_kembar(
        &baris
            .iter()
            .filter_map(|b| {
                Some(StockLine {
                    product_id: b.product_id?,
                    qty: b.qty,
                    batch_id: None,
                })
            })
            .collect::<Vec<_>>(),
    );

    let ticket_id = repo::insert_ticket(&mut tx, external_order_id, notes)
        .await?
        .ok_or_else(|| AppError::conflict("Pesanan ini sudah punya tiket packing."))?;

    for line in &lines {
        let nama_produk = nama
            .get(&line.product_id)
            .map(String::as_str)
            .unwrap_or("Produk tanpa nama");

        repo::insert_item(&mut tx, ticket_id, line.product_id, nama_produk, line.qty).await?;
    }

    tx.commit().await?;

    Ok(ticket_id)
}

/// Menugaskan tiket: Owner boleh siapa saja, pengepak hanya untuk dirinya agar tak melempar pekerjaan ke rekan yang tak tahu.
pub async fn tugaskan(
    pool: &PgPool,
    ticket_id: Uuid,
    kepada: Uuid,
    oleh: CurrentUser,
) -> AppResult<()> {
    if oleh.role != Role::Owner && kepada != oleh.id {
        return Err(AppError::forbidden(
            "Kamu hanya bisa mengambil tiket untuk dirimu sendiri.",
        ));
    }

    let mut tx = pool.begin().await?;
    let tiket = ambil_terkunci(&mut tx, ticket_id).await?;
    let status = TicketStatus::parse(&tiket.status)?;

    if !status.masih_bisa_ditugaskan() {
        return Err(AppError::conflict(
            "Tiket yang sudah selesai dikemas tidak bisa dipindahtangankan.",
        ));
    }

    if !repo::bisa_mengepak(&mut tx, kepada).await? {
        return Err(AppError::bad_request(
            "Tiket hanya bisa ditugaskan ke pengguna aktif berperan pengepak atau owner.",
        ));
    }

    repo::set_penugasan(&mut tx, ticket_id, kepada, oleh.id).await?;
    tx.commit().await?;

    Ok(())
}

/// Memindahkan status satu langkah; stok pesanan marketplace berkurang hanya saat `handed_over` (barang ke kurir), bukan lebih awal.
pub async fn pindah_status(
    pool: &PgPool,
    ticket_id: Uuid,
    tujuan: TicketStatus,
    oleh: CurrentUser,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let tiket = ambil_terkunci(&mut tx, ticket_id).await?;

    pastikan_boleh_mengerjakan(&tiket, oleh)?;
    TicketStatus::parse(&tiket.status)?.pindah_ke(tujuan)?;

    if tujuan == TicketStatus::Packed && repo::ada_yang_belum_dikemas(&mut tx, ticket_id).await? {
        return Err(AppError::conflict(
            "Masih ada barang yang belum dicentang sebagai sudah dikemas.",
        ));
    }

    if tujuan == TicketStatus::HandedOver {
        let lines: Vec<StockLine> = repo::baris_stok(&mut tx, ticket_id)
            .await?
            .into_iter()
            .map(|b| StockLine {
                product_id: b.product_id,
                qty: b.qty,
                // Pesanan marketplace tak lewat meja kasir sehingga tak ada yang memilih batch: FEFO satu-satunya aturan.
                batch_id: None,
            })
            .collect();

        // `reference_id` menunjuk pesanan, bukan tiket (pasangan `reference_type = 'external_order'`), yang dicari saat menelusuri kenapa stok berkurang.
        stock::kurangi(
            &mut tx,
            &lines,
            StockReason::ExternalOrder,
            tiket.external_order_id,
            Some(oleh.id),
        )
        .await?;
    }

    repo::set_status(
        &mut tx,
        ticket_id,
        tujuan.as_str(),
        tujuan == TicketStatus::HandedOver,
    )
    .await?;

    tx.commit().await?;

    Ok(())
}

/// Mencentang barang dikemas (atau membatalkan) hanya saat tiket dikerjakan; setelah `packed` dibekukan agar cek "semua barang dikemas" yang meloloskannya tak kehilangan arti.
pub async fn tandai_item(
    pool: &PgPool,
    ticket_id: Uuid,
    item_id: Uuid,
    is_packed: bool,
    oleh: CurrentUser,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let tiket = ambil_terkunci(&mut tx, ticket_id).await?;

    pastikan_boleh_mengerjakan(&tiket, oleh)?;

    if TicketStatus::parse(&tiket.status)? != TicketStatus::Packing {
        return Err(AppError::conflict(
            "Centang barang hanya bisa diubah saat tiket sedang dikemas.",
        ));
    }

    if !repo::set_item_terkemas(&mut tx, ticket_id, item_id, is_packed).await? {
        return Err(AppError::not_found("Barang tidak ada di tiket ini."));
    }

    tx.commit().await?;

    Ok(())
}

async fn ambil_terkunci(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_id: Uuid,
) -> AppResult<TiketTerkunci> {
    repo::kunci(tx, ticket_id)
        .await?
        .ok_or_else(|| AppError::not_found("Tiket tidak ditemukan."))
}

/// Yang boleh mengubah tiket hanya pengepak yang ditugaskan, dan Owner.
fn pastikan_boleh_mengerjakan(tiket: &TiketTerkunci, oleh: CurrentUser) -> AppResult<()> {
    if oleh.role == Role::Owner || tiket.assigned_to_user_id == Some(oleh.id) {
        return Ok(());
    }

    // Dua sebab berbeda dan pengepak hanya bisa berbuat pada yang pertama, jadi pesannya dibedakan.
    Err(AppError::forbidden(match tiket.assigned_to_user_id {
        None => "Ambil tiket ini dulu sebelum mengerjakannya.",
        Some(_) => "Tiket ini ditugaskan ke orang lain.",
    }))
}
