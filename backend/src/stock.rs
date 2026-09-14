//! Satu-satunya tempat stok produk boleh berubah.
//!
//! Di proyek lama logika ini ada dua salinan -- `commitCheckout()` untuk POS
//! dan `updateTicketProgress()` untuk serah-terima tiket -- dengan aturan
//! penguncian dan penulisan ledger yang sama, ditulis ulang. Dua salinan
//! berarti dua kesempatan untuk menyimpang. Di sini keduanya memanggil
//! fungsi yang sama.
//!
//! Aturan yang dijaga fungsi ini:
//!
//! 1. Baris produk dikunci `FOR UPDATE` setelah di-dedup dan diurutkan
//!    berdasarkan `id`. Urutan yang konsisten inilah yang mencegah deadlock
//!    saat dua transaksi menyentuh himpunan produk yang beririsan.
//! 2. SELURUH item diperiksa kecukupan stoknya SEBELUM satu baris pun
//!    ditulis. Jadi tidak mungkin ada keadaan setengah jadi: entah semua
//!    berhasil, atau tidak ada yang berubah sama sekali.
//! 3. Setiap perubahan menulis satu baris `stock_adjustments` berisi
//!    `stock_before` dan `stock_after` -- ledger yang hanya bertambah,
//!    tidak pernah diubah.
//!
//! Fungsi ini selalu menerima transaksi yang sudah dibuka pemanggilnya,
//! bukan membuka sendiri. Dengan begitu pengurangan stok dan perubahan yang
//! menyebabkannya (transaksi POS, status tiket) commit atau rollback
//! bersama-sama.

use crate::error::{AppError, AppResult};
use rust_decimal::Decimal;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Alasan stok berubah. Setiap alasan menentukan `reference_type`-nya
/// sekaligus, karena keduanya dibatasi CHECK constraint di database dan
/// pasangan yang salah baru ketahuan saat INSERT ditolak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockReason {
    /// Penjualan POS.
    Sale,
    /// Serah-terima tiket packing untuk order marketplace.
    ExternalOrder,
    /// Koreksi manual oleh Owner.
    ManualAdjustment,
    /// Barang masuk yang dicatat sebagai batch, lengkap dengan tanggal
    /// kedaluwarsanya. Dibedakan dari koreksi manual supaya ledger bisa
    /// menjawab "stok ini datang dari kiriman mana", bukan cuma "seseorang
    /// mengubahnya".
    Restock,
}

impl StockReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::ExternalOrder => "external_order",
            Self::ManualAdjustment => "manual_adjustment",
            Self::Restock => "restock",
        }
    }

    fn reference_type(self) -> &'static str {
        match self {
            Self::Sale => "transaction",
            Self::ExternalOrder => "external_order",
            // Batch masuk tidak berasal dari transaksi maupun pesanan
            // marketplace; "manual" adalah satu-satunya nilai yang
            // diizinkan CHECK constraint untuk asal seperti itu.
            Self::ManualAdjustment | Self::Restock => "manual",
        }
    }
}

/// Satu baris permintaan perubahan stok.
#[derive(Debug, Clone, Copy)]
pub struct StockLine {
    pub product_id: Uuid,
    /// Selalu positif. Arah perubahan ditentukan fungsi yang dipanggil.
    pub qty: i32,
}

/// Produk yang barisnya sedang terkunci.
///
/// `name` dan `price` ikut dibaca supaya pemanggil tidak perlu query kedua:
/// POS memakainya untuk menetapkan harga dan menyimpan snapshot nama di
/// baris transaksi, dan pesan "stok tidak cukup" menyebut namanya. Kalau
/// keduanya diambil terpisah, pemanggil akan tergoda menulis SELECT ... FOR
/// UPDATE sendiri -- dan salinan kedua aturan penguncian itulah yang justru
/// ingin dihindari modul ini.
#[derive(Debug)]
pub struct LockedProduct {
    pub id: Uuid,
    pub name: String,
    pub price: Decimal,
    pub stock_qty: i32,
}

/// Menggabungkan baris dengan produk yang sama.
///
/// Kasir yang memindai barang yang sama dua kali mengirim dua baris. Tanpa
/// digabung, pemeriksaan stok dilakukan per baris dan bisa lolos padahal
/// totalnya melebihi stok yang ada. `BTreeMap` sekaligus memberi urutan
/// berdasarkan id, yang dibutuhkan penguncian.
pub fn gabungkan_baris_kembar(lines: &[StockLine]) -> Vec<StockLine> {
    let mut per_produk: BTreeMap<Uuid, i32> = BTreeMap::new();
    for line in lines {
        *per_produk.entry(line.product_id).or_insert(0) += line.qty;
    }
    per_produk
        .into_iter()
        .map(|(product_id, qty)| StockLine { product_id, qty })
        .collect()
}

/// Mengunci baris produk yang akan diubah.
///
/// Urutan `ORDER BY id` bukan kosmetik: dua transaksi yang mengunci produk
/// A dan B dalam urutan berlawanan akan saling menunggu selamanya. Dengan
/// urutan yang sama di semua pemanggil, yang kedua cukup menunggu yang
/// pertama selesai.
pub async fn kunci_produk(
    tx: &mut Transaction<'_, Postgres>,
    product_ids: &[Uuid],
) -> AppResult<BTreeMap<Uuid, LockedProduct>> {
    let mut unik: Vec<Uuid> = product_ids.to_vec();
    unik.sort_unstable();
    unik.dedup();

    let rows = sqlx::query_as!(
        LockedProduct,
        r#"
        SELECT id, name, price, stock_qty
        FROM products
        WHERE id = ANY($1)
        ORDER BY id
        FOR UPDATE
        "#,
        &unik
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows.into_iter().map(|r| (r.id, r)).collect())
}

/// Mengurangi stok untuk sekumpulan item dan mencatatnya di ledger.
///
/// Baris kembar digabung lebih dulu, jadi pemanggil boleh mengirim apa
/// adanya. Mengembalikan `CONFLICT` kalau ada satu saja item yang stoknya
/// tidak cukup -- dan dalam hal itu tidak ada apa pun yang tertulis.
pub async fn kurangi(
    tx: &mut Transaction<'_, Postgres>,
    lines: &[StockLine],
    reason: StockReason,
    reference_id: Uuid,
    actor_user_id: Option<Uuid>,
) -> AppResult<()> {
    let lines = gabungkan_baris_kembar(lines);
    if lines.is_empty() {
        return Ok(());
    }

    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    let terkunci = kunci_produk(tx, &ids).await?;

    // Tahap 1 -- periksa semuanya. Belum ada yang ditulis.
    for line in &lines {
        let product = terkunci
            .get(&line.product_id)
            .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

        if product.stock_qty < line.qty {
            return Err(AppError::conflict(format!(
                "Stok \"{}\" tinggal {}, tidak cukup untuk {}.",
                product.name, product.stock_qty, line.qty
            )));
        }
    }

    // Tahap 2 -- baru menulis. Semua sudah dipastikan cukup di atas, jadi
    // tidak ada pemeriksaan yang bisa gagal di tengah jalan.
    for line in &lines {
        let product = &terkunci[&line.product_id];
        let stock_before = product.stock_qty;
        let stock_after = stock_before - line.qty;

        tulis_perubahan(
            tx,
            product.id,
            -line.qty,
            stock_before,
            stock_after,
            reason,
            reference_id,
            actor_user_id,
        )
        .await?;
    }

    Ok(())
}

/// Menambah stok. Dipakai pembatalan transaksi dan koreksi manual ke atas.
pub async fn tambah(
    tx: &mut Transaction<'_, Postgres>,
    lines: &[StockLine],
    reason: StockReason,
    reference_id: Uuid,
    actor_user_id: Option<Uuid>,
) -> AppResult<()> {
    let lines = gabungkan_baris_kembar(lines);
    if lines.is_empty() {
        return Ok(());
    }

    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    let terkunci = kunci_produk(tx, &ids).await?;

    for line in &lines {
        let product = terkunci
            .get(&line.product_id)
            .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

        let stock_before = product.stock_qty;
        let stock_after = stock_before + line.qty;

        tulis_perubahan(
            tx,
            product.id,
            line.qty,
            stock_before,
            stock_after,
            reason,
            reference_id,
            actor_user_id,
        )
        .await?;
    }

    Ok(())
}

/// Memperbarui `products.stock_qty` dan menulis satu baris ledger. Keduanya
/// selalu terjadi bersama -- stok yang berubah tanpa jejak di ledger membuat
/// audit tidak mungkin dilakukan.
#[allow(clippy::too_many_arguments)]
async fn tulis_perubahan(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    change_qty: i32,
    stock_before: i32,
    stock_after: i32,
    reason: StockReason,
    reference_id: Uuid,
    actor_user_id: Option<Uuid>,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE products SET stock_qty = $1, updated_at = now() WHERE id = $2",
        stock_after,
        product_id
    )
    .execute(&mut **tx)
    .await?;

    sqlx::query!(
        r#"
        INSERT INTO stock_adjustments
            (product_id, change_qty, reason, reference_type, reference_id,
             stock_before, stock_after, adjusted_by_user_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        product_id,
        change_qty,
        reason.as_str(),
        reason.reference_type(),
        reference_id,
        stock_before,
        stock_after,
        actor_user_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baris_produk_kembar_dijumlahkan() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);

        let hasil = gabungkan_baris_kembar(&[
            StockLine {
                product_id: a,
                qty: 2,
            },
            StockLine {
                product_id: b,
                qty: 1,
            },
            StockLine {
                product_id: a,
                qty: 3,
            },
        ]);

        assert_eq!(hasil.len(), 2);
        assert_eq!(hasil[0].product_id, a);
        assert_eq!(hasil[0].qty, 5);
        assert_eq!(hasil[1].qty, 1);
    }

    #[test]
    fn hasil_penggabungan_selalu_urut_berdasarkan_id() {
        // Urutan inilah yang mencegah deadlock saat mengunci baris produk,
        // jadi sifat ini harus tetap benar berapa pun urutan masukannya.
        let hasil = gabungkan_baris_kembar(&[
            StockLine {
                product_id: Uuid::from_u128(9),
                qty: 1,
            },
            StockLine {
                product_id: Uuid::from_u128(3),
                qty: 1,
            },
            StockLine {
                product_id: Uuid::from_u128(5),
                qty: 1,
            },
        ]);

        let ids: Vec<Uuid> = hasil.iter().map(|l| l.product_id).collect();
        let mut urut = ids.clone();
        urut.sort_unstable();
        assert_eq!(ids, urut);
    }

    #[test]
    fn alasan_dan_reference_type_berpasangan_sesuai_check_constraint() {
        assert_eq!(StockReason::Sale.reference_type(), "transaction");
        assert_eq!(
            StockReason::ExternalOrder.reference_type(),
            "external_order"
        );
        assert_eq!(StockReason::ManualAdjustment.reference_type(), "manual");
        assert_eq!(StockReason::Restock.as_str(), "restock");
        assert_eq!(StockReason::Restock.reference_type(), "manual");
    }
}
