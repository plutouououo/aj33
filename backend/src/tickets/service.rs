//! Aturan tiket packing: semua perubahan satu transaksi database diawali `repo::kunci()` karena status tanpa kunci bisa basi dan serah-terima (mengurangi stok) paling sering ditekan dua kali.

use super::repo::{self, TiketTerkunci};
use super::TicketStatus;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::pos::service::{
    self as pos, CheckoutInput, CheckoutItem, PaymentMethod, SalesChannel, TransactionType,
};
use crate::stock::{self, StockLine};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

pub async fn buat(pool: &PgPool, external_order_id: Uuid, notes: Option<&str>) -> AppResult<Uuid> {
    let mut tx = pool.begin().await?;
    let ticket_id = buat_dalam(&mut tx, external_order_id, notes, &HashMap::new()).await?;
    tx.commit().await?;

    Ok(ticket_id)
}

/// Satu pesanan yang dikirim owner beserta batch pilihannya per produk; produk tanpa pilihan diambil FEFO.
pub struct PesananKirim {
    pub external_order_id: Uuid,
    pub batches: HashMap<Uuid, Uuid>,
}

/// Mengirim sekumpulan pesanan sebagai tiket ke satu pengepak sekaligus; semuanya atau tidak sama sekali, agar owner tak perlu menebak pesanan mana yang sudah terkirim.
pub async fn kirim(
    pool: &PgPool,
    pesanan: &[PesananKirim],
    kepada: Uuid,
    oleh: CurrentUser,
) -> AppResult<Vec<Uuid>> {
    if pesanan.is_empty() {
        return Err(AppError::bad_request("Pilih minimal satu pesanan."));
    }

    let mut tx = pool.begin().await?;

    if !repo::bisa_mengepak(&mut tx, kepada).await? {
        return Err(AppError::bad_request(
            "Tugas packing hanya bisa dikirim ke pengguna aktif berperan pengepak atau owner.",
        ));
    }

    let mut tiket = Vec::with_capacity(pesanan.len());
    for p in pesanan {
        let id = buat_dalam(&mut tx, p.external_order_id, None, &p.batches).await?;
        repo::set_penugasan(&mut tx, id, kepada, oleh.id).await?;
        repo::set_status_pesanan(&mut tx, p.external_order_id, "processing").await?;
        tiket.push(id);
    }

    tx.commit().await?;

    Ok(tiket)
}

/// Membuat tiket pesanan marketplace; barang yang listing-nya belum dipetakan membuatnya DITOLAK agar paket tak kurang, dan Owner diberi tahu barang yang perlu dipetakan.
async fn buat_dalam(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
    notes: Option<&str>,
    batches: &HashMap<Uuid, Uuid>,
) -> AppResult<Uuid> {
    if !repo::ada_pesanan(tx, external_order_id).await? {
        return Err(AppError::not_found("Pesanan tidak ditemukan."));
    }

    let baris = repo::baris_pesanan(tx, external_order_id).await?;
    if baris.is_empty() {
        return Err(AppError::conflict(
            "Pesanan ini belum punya rincian barang, jadi tugas packing belum bisa dibuat.",
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
                let product_id = b.product_id?;
                Some(StockLine {
                    product_id,
                    qty: b.qty,
                    batch_id: batches.get(&product_id).copied(),
                })
            })
            .collect::<Vec<_>>(),
    );

    let ticket_id = repo::insert_ticket(tx, external_order_id, notes)
        .await?
        .ok_or_else(|| AppError::conflict("Pesanan ini sudah punya tugas packing."))?;

    for line in &lines {
        let nama_produk = nama
            .get(&line.product_id)
            .map(String::as_str)
            .unwrap_or("Produk tanpa nama");

        if let Some(batch_id) = line.batch_id {
            if !repo::batch_milik_produk(tx, batch_id, line.product_id).await? {
                return Err(AppError::bad_request(format!(
                    "Batch yang dipilih bukan milik \"{nama_produk}\"."
                )));
            }
        }

        repo::insert_item(
            tx,
            ticket_id,
            line.product_id,
            nama_produk,
            line.qty,
            line.batch_id,
        )
        .await?;
    }

    Ok(ticket_id)
}

/// Menugaskan tiket; hanya Owner yang membagi pekerjaan, pengepak tinggal mengerjakan yang diberikan kepadanya.
pub async fn tugaskan(
    pool: &PgPool,
    ticket_id: Uuid,
    kepada: Uuid,
    oleh: CurrentUser,
) -> AppResult<()> {
    if oleh.role != Role::Owner {
        return Err(AppError::forbidden(
            "Hanya owner yang bisa menugaskan tugas packing.",
        ));
    }

    let mut tx = pool.begin().await?;
    let tiket = ambil_terkunci(&mut tx, ticket_id).await?;
    let status = TicketStatus::parse(&tiket.status)?;

    if !status.masih_bisa_ditugaskan() {
        return Err(AppError::conflict(
            "Tugas packing yang sudah selesai dikemas tidak bisa dipindahtangankan.",
        ));
    }

    if !repo::bisa_mengepak(&mut tx, kepada).await? {
        return Err(AppError::bad_request(
            "Tugas packing hanya bisa ditugaskan ke pengguna aktif berperan pengepak atau owner.",
        ));
    }

    repo::set_penugasan(&mut tx, ticket_id, kepada, oleh.id).await?;
    tx.commit().await?;

    Ok(())
}

/// Memindahkan status satu langkah; pesanan baru tercatat sebagai penjualan dan stoknya berkurang saat `handed_over` (barang keluar), bukan lebih awal.
pub async fn pindah_status(
    pool: &PgPool,
    ticket_id: Uuid,
    tujuan: TicketStatus,
    oleh: CurrentUser,
) -> AppResult<()> {
    // ACC owner terjadi di WhatsApp, di luar aplikasi, jadi hanya owner yang tahu kapan barang boleh keluar.
    if tujuan == TicketStatus::HandedOver && oleh.role != Role::Owner {
        return Err(AppError::forbidden(
            "Hanya owner yang bisa menyelesaikan tugas packing, setelah ACC di WhatsApp.",
        ));
    }

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
        catat_penjualan(pool, &mut tx, tiket.external_order_id, ticket_id, oleh).await?;
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
            "Centang barang hanya bisa diubah saat tugas packing sedang dikemas.",
        ));
    }

    if !repo::set_item_terkemas(&mut tx, ticket_id, item_id, is_packed).await? {
        return Err(AppError::not_found(
            "Barang tidak ada di tugas packing ini.",
        ));
    }

    tx.commit().await?;

    Ok(())
}

/// Penjualan lewat jalur kasir yang sama (harga kanal, potongan Shopee, ledger stok) agar laporan, void, dan riwayat pelanggan memperlakukannya seperti transaksi lain; kunci idempotensinya id pesanan.
async fn catat_penjualan(
    pool: &PgPool,
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
    ticket_id: Uuid,
    oleh: CurrentUser,
) -> AppResult<()> {
    let pesanan = repo::info_pesanan(tx, external_order_id).await?;
    let sales_channel = match pesanan.platform_name.as_str() {
        "shopee" => SalesChannel::Shopee,
        "tiktok" | "tokopedia" => SalesChannel::Tiktok,
        lain => {
            return Err(AppError::conflict(format!(
                "Pesanan platform \"{lain}\" belum bisa dicatat sebagai penjualan."
            )))
        }
    };

    let items = repo::baris_stok(tx, ticket_id)
        .await?
        .into_iter()
        .map(|b| CheckoutItem {
            product_id: b.product_id,
            qty: b.qty,
            batch_id: b.batch_id,
        })
        .collect();

    let transaction_id = pos::catat_dari_pesanan(
        pool,
        tx,
        &CheckoutInput {
            idempotency_key: format!("pesanan:{external_order_id}"),
            transaction_type: TransactionType::PreOrder,
            customer_id: pesanan.customer_id,
            payment_method: PaymentMethod::Ewallet,
            sales_channel,
            amount_paid: None,
            shipping_cost: None,
            shipping_borne_by_store: false,
            platform_commission_fee_percent: None,
            platform_service_fee_percent: None,
            items,
            cashier_user_id: oleh.id,
        },
    )
    .await?;

    repo::tautkan_penjualan(tx, external_order_id, transaction_id).await
}

async fn ambil_terkunci(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ticket_id: Uuid,
) -> AppResult<TiketTerkunci> {
    repo::kunci(tx, ticket_id)
        .await?
        .ok_or_else(|| AppError::not_found("Tugas packing tidak ditemukan."))
}

/// Yang boleh mengubah tiket hanya pengepak yang ditugaskan, dan Owner.
fn pastikan_boleh_mengerjakan(tiket: &TiketTerkunci, oleh: CurrentUser) -> AppResult<()> {
    if oleh.role == Role::Owner || tiket.assigned_to_user_id == Some(oleh.id) {
        return Ok(());
    }

    // Dua sebab berbeda dan pengepak hanya bisa berbuat pada yang pertama, jadi pesannya dibedakan.
    Err(AppError::forbidden(match tiket.assigned_to_user_id {
        None => "Tugas packing ini belum ditugaskan ke siapa pun.",
        Some(_) => "Tugas packing ini ditugaskan ke orang lain.",
    }))
}
