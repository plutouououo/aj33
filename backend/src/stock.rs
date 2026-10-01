//! Satu-satunya tempat stok berubah (invarian `products.stock_qty = SUM(remaining_qty)`): produk dikunci berurut id, semua item dicek sebelum menulis, tiap perubahan masuk ledger `stock_adjustments`.

use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Alasan stok berubah menentukan `reference_type`, keduanya dibatasi CHECK sehingga pasangan salah baru ketahuan saat INSERT ditolak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockReason {
    /// Penjualan POS.
    Sale,
    /// Serah-terima tiket packing untuk order marketplace.
    ExternalOrder,
    /// Koreksi manual oleh Owner.
    ManualAdjustment,
    /// Barang masuk sebagai batch lengkap dengan kedaluwarsa, dibedakan dari koreksi manual agar ledger menjawab "datang dari kiriman mana".
    Restock,
    /// Pengembalian stok karena void, dibedakan dari `ManualAdjustment` agar ledger menjawab "kembali karena transaksi apa".
    VoidReversal,
}

impl StockReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::ExternalOrder => "external_order",
            Self::ManualAdjustment => "manual_adjustment",
            Self::Restock => "restock",
            Self::VoidReversal => "void_reversal",
        }
    }

    fn reference_type(self) -> &'static str {
        match self {
            // Void mengacu balik ke transaksi yang dibatalkan, sama seperti penjualan aslinya.
            Self::Sale | Self::VoidReversal => "transaction",
            Self::ExternalOrder => "external_order",
            // Batch masuk tak berasal dari transaksi/pesanan; "manual" satu-satunya nilai CHECK untuk asal seperti itu.
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
    /// Batch pilihan kasir; `None` = FEFO, tapi pilihan manual perlu karena FEFO tak cocok untuk semua penjualan; yang penting batch yang keluar tercatat.
    pub batch_id: Option<Uuid>,
}

/// Produk yang barisnya terkunci; `name` dan `price` ikut dibaca agar pemanggil tak menulis `SELECT ... FOR UPDATE` sendiri (salinan kedua aturan penguncian).
#[derive(Debug)]
pub struct LockedProduct {
    pub id: Uuid,
    pub name: String,
    /// Isi satu pack dalam kg; bersama `price_wholesale` menentukan ecer atau grosir di kanal toko.
    pub variant_size: Option<Decimal>,
    pub price: Decimal,
    /// `None` = grosir belum diatur, kasir tetap memakai `price`.
    pub price_wholesale: Option<Decimal>,
    /// Harga kanal dibaca dari baris terkunci yang sama agar harga tak bisa berubah antara dua bacaan; `None` = kanal belum diatur, jatuh ke `price`.
    pub price_shopee: Option<Decimal>,
    /// Mencakup Tokopedia; satu kanal dengan TikTok Shop.
    pub price_tiktok: Option<Decimal>,
    pub stock_qty: i32,
}

/// Batch yang barisnya sedang terkunci.
#[derive(Debug)]
struct LockedBatch {
    id: Uuid,
    batch_number: Option<String>,
    expiry_date: Option<NaiveDate>,
    remaining_qty: i32,
}

impl LockedBatch {
    /// Sebutan batch untuk pesan galat: nomor batch atau tanggal kedaluwarsa, lebih berguna di depan rak daripada UUID.
    fn label(&self) -> String {
        match (&self.batch_number, self.expiry_date) {
            (Some(nomor), _) => nomor.clone(),
            (None, Some(exp)) => format!("kedaluwarsa {exp}"),
            (None, None) => "tanpa nomor".to_string(),
        }
    }
}

/// Satu potong alokasi: sekian butir diambil dari satu batch tertentu.
#[derive(Debug)]
struct Alokasi {
    product_id: Uuid,
    batch_id: Uuid,
    qty: i32,
}

/// Menggabungkan baris produk DAN batch sama (pemindaian ganda) agar cek stok tak lolos; `BTreeMap` memberi urutan id untuk penguncian.
pub fn gabungkan_baris_kembar(lines: &[StockLine]) -> Vec<StockLine> {
    let mut per_baris: BTreeMap<(Uuid, Option<Uuid>), i32> = BTreeMap::new();
    for line in lines {
        *per_baris
            .entry((line.product_id, line.batch_id))
            .or_insert(0) += line.qty;
    }
    per_baris
        .into_iter()
        .map(|((product_id, batch_id), qty)| StockLine {
            product_id,
            qty,
            batch_id,
        })
        .collect()
}

/// Total per produk mengabaikan batch, agar satu produk tetap satu baris struk walau diambil dari dua batch.
pub fn total_per_produk(lines: &[StockLine]) -> Vec<(Uuid, i32)> {
    let mut per_produk: BTreeMap<Uuid, i32> = BTreeMap::new();
    for line in lines {
        *per_produk.entry(line.product_id).or_insert(0) += line.qty;
    }
    per_produk.into_iter().collect()
}

/// Mengunci baris produk dengan `ORDER BY id` (bukan kosmetik): urutan berlawanan membuat dua transaksi saling menunggu selamanya.
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
        SELECT id, name, variant_size, price, price_wholesale, price_shopee, price_tiktok, stock_qty
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

/// Batch urut FEFO dengan `NULLS LAST`: tanpa tanggal berarti tak diketahui (bukan tak terhingga), jadi barang bertanggal jelas diprioritaskan.
async fn kunci_batch_fefo(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
) -> AppResult<Vec<LockedBatch>> {
    let rows = sqlx::query_as!(
        LockedBatch,
        r#"
        SELECT id, batch_number, expiry_date, remaining_qty
        FROM product_batches
        WHERE product_id = $1 AND remaining_qty > 0
        ORDER BY expiry_date NULLS LAST, received_at, id
        FOR UPDATE
        "#,
        product_id
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}

/// Satu batch tertentu milik produk tertentu; `product_id` ikut syarat agar batch milik produk lain tak diam-diam dilayani.
async fn kunci_batch_tunggal(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    batch_id: Uuid,
) -> AppResult<Option<LockedBatch>> {
    let row = sqlx::query_as!(
        LockedBatch,
        r#"
        SELECT id, batch_number, expiry_date, remaining_qty
        FROM product_batches
        WHERE id = $1 AND product_id = $2
        FOR UPDATE
        "#,
        batch_id,
        product_id
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row)
}

/// Merencanakan dari batch mana baris diambil; `sudah` mencatat butir yang dijanjikan ke baris lain agar dua baris produk sama tak menjanjikan butir yang sama.
async fn rencanakan(
    tx: &mut Transaction<'_, Postgres>,
    product: &LockedProduct,
    line: &StockLine,
    sudah: &mut BTreeMap<Uuid, i32>,
) -> AppResult<Vec<Alokasi>> {
    match line.batch_id {
        // Kasir memilih sendiri, tanpa jatuh-balik ke FEFO bila batch kurang: mengambil dari batch lain berarti barang keluar tak sesuai catatan.
        Some(batch_id) => {
            let batch = kunci_batch_tunggal(tx, product.id, batch_id)
                .await?
                .ok_or_else(|| {
                    AppError::not_found(format!(
                        "Batch yang dipilih tidak ada pada produk \"{}\".",
                        product.name
                    ))
                })?;

            let tersedia = batch.remaining_qty - sudah.get(&batch_id).copied().unwrap_or(0);
            if tersedia < line.qty {
                return Err(AppError::conflict(format!(
                    "Batch {} untuk \"{}\" tinggal {}, tidak cukup untuk {}.",
                    batch.label(),
                    product.name,
                    tersedia.max(0),
                    line.qty
                )));
            }

            *sudah.entry(batch_id).or_insert(0) += line.qty;
            Ok(vec![Alokasi {
                product_id: product.id,
                batch_id,
                qty: line.qty,
            }])
        }

        None => {
            let batches = kunci_batch_fefo(tx, product.id).await?;
            let mut sisa = line.qty;
            let mut hasil = Vec::new();

            for batch in batches {
                if sisa == 0 {
                    break;
                }
                let tersedia = batch.remaining_qty - sudah.get(&batch.id).copied().unwrap_or(0);
                if tersedia <= 0 {
                    continue;
                }

                let ambil = tersedia.min(sisa);
                *sudah.entry(batch.id).or_insert(0) += ambil;
                hasil.push(Alokasi {
                    product_id: product.id,
                    batch_id: batch.id,
                    qty: ambil,
                });
                sisa -= ambil;
            }

            if sisa > 0 {
                // Invarian stok = jumlah sisa batch dijaga migrasi dan fungsi di berkas ini, jadi sampai di sini hanya bila ada yang menulis di luar modul; dijawab konflik dengan angka apa adanya, bukan panic.
                return Err(AppError::conflict(format!(
                    "Stok \"{}\" yang tercatat di batch kurang {} butir dari yang diminta. \
                     Periksa batch produk ini.",
                    product.name, sisa
                )));
            }

            Ok(hasil)
        }
    }
}

/// Mengurangi stok sekumpulan item dan mencatat di ledger; baris kembar digabung dulu, dan `CONFLICT` bila satu item pun tak cukup (tanpa tulisan apa pun).
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

    // Tahap 1: periksa kecukupan per produk, bukan per baris (dua baris bisa masing-masing muat tapi bersama melebihi stok); belum ada yang ditulis.
    for (product_id, total) in total_per_produk(&lines) {
        let product = terkunci
            .get(&product_id)
            .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

        if product.stock_qty < total {
            return Err(AppError::conflict(format!(
                "Stok \"{}\" tinggal {}, tidak cukup untuk {}.",
                product.name, product.stock_qty, total
            )));
        }
    }

    // Tahap 2: rencanakan alokasi batch; yang menyebut batch sendiri didahulukan agar pilihan kasir tak dihabiskan FEFO baris lain.
    let mut sudah: BTreeMap<Uuid, i32> = BTreeMap::new();
    let mut rencana: Vec<Alokasi> = Vec::new();

    for line in lines.iter().filter(|l| l.batch_id.is_some()) {
        let product = &terkunci[&line.product_id];
        rencana.extend(rencanakan(tx, product, line, &mut sudah).await?);
    }
    for line in lines.iter().filter(|l| l.batch_id.is_none()) {
        let product = &terkunci[&line.product_id];
        rencana.extend(rencanakan(tx, product, line, &mut sudah).await?);
    }

    // Tahap 3: baru menulis; semua sudah dipastikan cukup sehingga tak ada pemeriksaan yang gagal di tengah.
    let mut berjalan: BTreeMap<Uuid, i32> =
        terkunci.iter().map(|(id, p)| (*id, p.stock_qty)).collect();

    for alokasi in &rencana {
        let stock_before = berjalan[&alokasi.product_id];
        let stock_after = stock_before - alokasi.qty;

        ubah_sisa_batch(tx, alokasi.batch_id, -alokasi.qty).await?;
        tulis_perubahan(
            tx,
            alokasi.product_id,
            -alokasi.qty,
            stock_before,
            stock_after,
            reason,
            reference_id,
            Some(alokasi.batch_id),
            actor_user_id,
        )
        .await?;

        berjalan.insert(alokasi.product_id, stock_after);
    }

    Ok(())
}

/// Menambah stok selalu mendarat di sebuah batch; tanpa `batch_id` dibuatkan batch tanpa nomor/kedaluwarsa agar stok punya tempat dan FEFO tak kehilangan jejak.
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

    let mut berjalan: BTreeMap<Uuid, i32> =
        terkunci.iter().map(|(id, p)| (*id, p.stock_qty)).collect();

    for line in &lines {
        let product = terkunci
            .get(&line.product_id)
            .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

        let batch_id = match line.batch_id {
            Some(batch_id) => {
                kunci_batch_tunggal(tx, product.id, batch_id)
                    .await?
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "Batch yang dituju tidak ada pada produk \"{}\".",
                            product.name
                        ))
                    })?;
                batch_id
            }
            None => buat_batch_tanpa_asal(tx, product.id, line.qty).await?,
        };

        let stock_before = berjalan[&product.id];
        let stock_after = stock_before + line.qty;

        ubah_sisa_batch(tx, batch_id, line.qty).await?;
        tulis_perubahan(
            tx,
            product.id,
            line.qty,
            stock_before,
            stock_after,
            reason,
            reference_id,
            Some(batch_id),
            actor_user_id,
        )
        .await?;

        berjalan.insert(product.id, stock_after);
    }

    Ok(())
}

/// Batch penampung stok tanpa kiriman (selisih opname, barang kembali); `remaining_qty` mulai nol dan dinaikkan `ubah_sisa_batch` sehingga hanya satu penulis kolom itu.
async fn buat_batch_tanpa_asal(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    quantity: i32,
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO product_batches (product_id, batch_number, quantity, remaining_qty, expiry_date)
        VALUES ($1, NULL, $2, 0, NULL)
        RETURNING id
        "#,
        product_id,
        quantity
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

/// Menggeser sisa satu batch (`delta` negatif mengurangi); CHECK `product_batches_remaining_check` menolak hasil di luar 0..quantity sehingga salah hitung berhenti sebagai galat transaksi.
async fn ubah_sisa_batch(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    delta: i32,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE product_batches SET remaining_qty = remaining_qty + $1 WHERE id = $2",
        delta,
        batch_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Memperbarui `products.stock_qty` dan menulis satu baris ledger bersama, karena stok berubah tanpa jejak membuat audit mustahil.
#[allow(clippy::too_many_arguments)]
async fn tulis_perubahan(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    change_qty: i32,
    stock_before: i32,
    stock_after: i32,
    reason: StockReason,
    reference_id: Uuid,
    batch_id: Option<Uuid>,
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
             stock_before, stock_after, batch_id, adjusted_by_user_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        product_id,
        change_qty,
        reason.as_str(),
        reason.reference_type(),
        reference_id,
        stock_before,
        stock_after,
        batch_id,
        actor_user_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baris(product_id: Uuid, qty: i32) -> StockLine {
        StockLine {
            product_id,
            qty,
            batch_id: None,
        }
    }

    #[test]
    fn baris_produk_kembar_dijumlahkan() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);

        let hasil = gabungkan_baris_kembar(&[baris(a, 2), baris(b, 1), baris(a, 3)]);

        assert_eq!(hasil.len(), 2);
        assert_eq!(hasil[0].product_id, a);
        assert_eq!(hasil[0].qty, 5);
        assert_eq!(hasil[1].qty, 1);
    }

    #[test]
    fn batch_berbeda_tidak_digabung() {
        // Digabung akan menghapus pilihan kasir: dua batch yang sengaja dipilih tetap dua pengambilan.
        let p = Uuid::from_u128(1);
        let b1 = Uuid::from_u128(10);
        let b2 = Uuid::from_u128(11);

        let hasil = gabungkan_baris_kembar(&[
            StockLine {
                product_id: p,
                qty: 2,
                batch_id: Some(b1),
            },
            StockLine {
                product_id: p,
                qty: 3,
                batch_id: Some(b2),
            },
            StockLine {
                product_id: p,
                qty: 1,
                batch_id: Some(b1),
            },
        ]);

        assert_eq!(hasil.len(), 2);
        assert_eq!(hasil[0].batch_id, Some(b1));
        assert_eq!(hasil[0].qty, 3);
        assert_eq!(hasil[1].batch_id, Some(b2));
        assert_eq!(hasil[1].qty, 3);
    }

    #[test]
    fn total_per_produk_mengabaikan_batch() {
        // Satu produk tetap satu baris struk walau diambil dari dua batch.
        let p = Uuid::from_u128(1);
        let q = Uuid::from_u128(2);

        let total = total_per_produk(&[
            StockLine {
                product_id: p,
                qty: 2,
                batch_id: Some(Uuid::from_u128(10)),
            },
            StockLine {
                product_id: p,
                qty: 3,
                batch_id: None,
            },
            baris(q, 4),
        ]);

        assert_eq!(total, vec![(p, 5), (q, 4)]);
    }

    #[test]
    fn hasil_penggabungan_selalu_urut_berdasarkan_id() {
        // Urutan ini mencegah deadlock saat mengunci produk, jadi harus benar berapa pun urutan masukan.
        let hasil = gabungkan_baris_kembar(&[
            baris(Uuid::from_u128(9), 1),
            baris(Uuid::from_u128(3), 1),
            baris(Uuid::from_u128(5), 1),
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
        assert_eq!(StockReason::VoidReversal.as_str(), "void_reversal");
        assert_eq!(StockReason::VoidReversal.reference_type(), "transaction");
    }
}
