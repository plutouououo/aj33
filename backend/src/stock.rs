//! Satu-satunya tempat stok produk boleh berubah.
//!
//! Di proyek lama logika ini ada dua salinan -- `commitCheckout()` untuk POS
//! dan `updateTicketProgress()` untuk serah-terima tiket -- dengan aturan
//! penguncian dan penulisan ledger yang sama, ditulis ulang. Dua salinan
//! berarti dua kesempatan untuk menyimpang. Di sini keduanya memanggil
//! fungsi yang sama.
//!
//! STOK ADA DI BATCH. Sejak migrasi 0011, stok sungguhan tersimpan sebagai
//! `product_batches.remaining_qty`, dan `products.stock_qty` adalah
//! ringkasannya. Invarian yang dijaga modul ini:
//!
//!     products.stock_qty = SUM(product_batches.remaining_qty) per produk
//!
//! Setiap penambahan stok masuk ke sebuah batch, dan setiap pengurangan
//! keluar dari batch tertentu -- baik yang dipilih kasir maupun yang dipilih
//! FEFO. Karena itu tidak ada butir stok yang tidak diketahui asal dan
//! kedaluwarsanya.
//!
//! Aturan yang dijaga fungsi ini:
//!
//! 1. Baris produk dikunci `FOR UPDATE` setelah di-dedup dan diurutkan
//!    berdasarkan `id`. Urutan yang konsisten inilah yang mencegah deadlock
//!    saat dua transaksi menyentuh himpunan produk yang beririsan. Baris
//!    batch dikunci SETELAH produknya; karena produk sudah terkunci, tidak
//!    ada dua transaksi yang bisa berebut batch produk yang sama.
//! 2. SELURUH item diperiksa kecukupan stoknya, lalu SELURUH alokasi batch
//!    direncanakan, SEBELUM satu baris pun ditulis. Jadi tidak mungkin ada
//!    keadaan setengah jadi: entah semua berhasil, atau tidak ada yang
//!    berubah sama sekali.
//! 3. Setiap perubahan menulis satu baris `stock_adjustments` berisi
//!    `stock_before`, `stock_after`, dan `batch_id` -- ledger yang hanya
//!    bertambah, tidak pernah diubah. Satu item yang mengambil dari dua
//!    batch menulis dua baris; itu memang dua kejadian.
//!
//! Fungsi ini selalu menerima transaksi yang sudah dibuka pemanggilnya,
//! bukan membuka sendiri. Dengan begitu pengurangan stok dan perubahan yang
//! menyebabkannya (transaksi POS, status tiket) commit atau rollback
//! bersama-sama.

use crate::error::{AppError, AppResult};
use chrono::NaiveDate;
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
    /// Batch yang dipilih kasir. `None` berarti serahkan pada FEFO --
    /// kedaluwarsa terdekat keluar lebih dulu.
    ///
    /// Pilihan manual ada karena FEFO adalah aturan yang benar untuk hampir
    /// semua penjualan, tapi bukan untuk semuanya: pembeli yang meminta
    /// barang untuk stok sendiri berhak mendapat yang kedaluwarsanya jauh,
    /// dan kasir yang mengambil fisik dari rak lain harus bisa mencatat apa
    /// yang benar-benar dia ambil. Yang penting bukan memaksa FEFO,
    /// melainkan bahwa batch mana pun yang keluar TERCATAT.
    pub batch_id: Option<Uuid>,
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
    /// Harga kanal, ikut terbaca di sini supaya penetapan harga saat
    /// checkout memakai baris yang SAMA yang sedang terkunci. Membacanya
    /// lewat query kedua berarti harga bisa berubah di antara dua bacaan,
    /// dan yang tercetak di struk bukan yang dipakai memeriksa stok.
    /// `None` berarti kanalnya belum diatur -- jatuh ke `price`.
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
    /// Sebutan batch untuk pesan galat. Nomor batch kalau ada, kalau tidak
    /// tanggal kedaluwarsanya -- keduanya lebih berguna bagi kasir yang
    /// sedang berdiri di depan rak daripada UUID.
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

/// Menggabungkan baris yang produk DAN batch-nya sama.
///
/// Kasir yang memindai barang yang sama dua kali mengirim dua baris. Tanpa
/// digabung, pemeriksaan stok dilakukan per baris dan bisa lolos padahal
/// totalnya melebihi stok yang ada. `BTreeMap` sekaligus memberi urutan
/// berdasarkan id produk, yang dibutuhkan penguncian.
///
/// Baris dengan produk sama tapi batch berbeda TIDAK digabung: keduanya
/// permintaan yang berbeda, dan menggabungkannya akan menghapus pilihan
/// kasir.
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

/// Total per produk, mengabaikan batch. Dipakai pemanggil yang menghitung
/// harga dan menyimpan baris transaksi: satu produk tetap satu baris di
/// struk walaupun barangnya diambil dari dua batch.
pub fn total_per_produk(lines: &[StockLine]) -> Vec<(Uuid, i32)> {
    let mut per_produk: BTreeMap<Uuid, i32> = BTreeMap::new();
    for line in lines {
        *per_produk.entry(line.product_id).or_insert(0) += line.qty;
    }
    per_produk.into_iter().collect()
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
        SELECT id, name, price, price_shopee, price_tiktok, stock_qty
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

/// Batch satu produk dalam urutan FEFO: kedaluwarsa terdekat lebih dulu.
///
/// `NULLS LAST` disengaja. Batch tanpa tanggal kedaluwarsa bukan batch yang
/// "kedaluwarsa tak terhingga", melainkan batch yang tanggalnya TIDAK
/// DIKETAHUI -- termasuk saldo awal bentukan migrasi 0011. Mendahulukannya
/// berarti menebak, sedangkan menaruhnya di belakang hanya berarti barang
/// yang tanggalnya jelas diprioritaskan keluar. Yang kedua bisa
/// dipertanggungjawabkan; yang pertama tidak.
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

/// Satu batch tertentu milik satu produk tertentu.
///
/// `product_id` ikut jadi syarat, bukan hanya `batch_id`: tanpa itu,
/// permintaan yang menyebut batch milik produk lain akan diam-diam dilayani,
/// dan stok dua produk berpindah tanpa ada yang tahu.
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

/// Merencanakan dari batch mana saja satu baris permintaan akan diambil.
///
/// `sudah` mencatat butir yang sudah dijanjikan ke baris lain dalam
/// perencanaan yang sama. Tanpa itu, dua baris untuk produk yang sama
/// (mis. satu memilih batch tertentu, satu lagi menyerah pada FEFO) akan
/// sama-sama melihat sisa yang belum berkurang dan menjanjikan butir yang
/// sama dua kali.
async fn rencanakan(
    tx: &mut Transaction<'_, Postgres>,
    product: &LockedProduct,
    line: &StockLine,
    sudah: &mut BTreeMap<Uuid, i32>,
) -> AppResult<Vec<Alokasi>> {
    match line.batch_id {
        // Kasir memilih sendiri. Tidak ada jatuh-balik ke FEFO kalau
        // batch-nya kurang: diam-diam mengambil dari batch lain berarti
        // barang yang keluar dari gudang bukan barang yang tercatat keluar.
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
                // Invarian stok = jumlah sisa batch sudah dijaga migrasi dan
                // seluruh fungsi di berkas ini, jadi sampai di sini hanya
                // mungkin kalau ada yang menulis ke tabel di luar modul ini.
                // Dijawab sebagai konflik dengan angka apa adanya, bukan
                // panic: kasir tidak bisa berbuat apa-apa dengan panic.
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

    // Tahap 1 -- periksa kecukupan per PRODUK, bukan per baris. Dua baris
    // untuk produk yang sama bisa masing-masing muat tapi bersama-sama
    // melebihi stok. Belum ada yang ditulis.
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

    // Tahap 2 -- rencanakan alokasi batch. Yang menyebut batch sendiri
    // didahulukan supaya pilihan kasir tidak keburu dihabiskan FEFO milik
    // baris lain untuk produk yang sama.
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

    // Tahap 3 -- baru menulis. Semua sudah dipastikan cukup di atas, jadi
    // tidak ada pemeriksaan yang bisa gagal di tengah jalan.
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

/// Menambah stok. Dipakai pencatatan batch baru dan koreksi manual ke atas.
///
/// Setiap penambahan harus mendarat di sebuah batch, karena di situlah stok
/// sungguhan disimpan. Kalau pemanggil menyebut batch (`batch_id`), ke sana;
/// kalau tidak -- koreksi manual hasil opname, misalnya -- dibuatkan batch
/// tanpa nomor dan tanpa tanggal kedaluwarsa. Batch bentukan itu bukan basa-
/// basi administratif: ia yang menjaga agar stok yang muncul entah dari mana
/// tetap punya tempat, dan agar FEFO tidak pernah kehilangan jejak butir.
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

/// Batch penampung untuk stok yang bertambah tanpa kiriman: selisih opname,
/// barang kembali, dan sejenisnya.
///
/// `remaining_qty` mulai dari nol dan dinaikkan `ubah_sisa_batch` seperti
/// batch lain, jadi hanya ada SATU tempat yang menulis kolom itu.
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

/// Menggeser sisa satu batch. `delta` negatif mengurangi.
///
/// CHECK constraint `product_batches_remaining_check` di database menolak
/// hasil di luar 0..quantity, jadi kesalahan hitung di sini berhenti sebagai
/// galat transaksi -- bukan sebagai sisa negatif yang menetap di tabel.
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
        // Menggabungkannya akan menghapus pilihan kasir: dua batch yang
        // dipilih sengaja harus tetap keluar sebagai dua pengambilan.
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
        // Satu produk tetap satu baris di struk walaupun diambil dari dua
        // batch -- pembeli membeli barang, bukan kiriman.
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
        // Urutan inilah yang mencegah deadlock saat mengunci baris produk,
        // jadi sifat ini harus tetap benar berapa pun urutan masukannya.
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
    }
}
