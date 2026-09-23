//! Akses tabel `products`, `categories`, dan `product_batches`.

use crate::error::{AppError, AppResult};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::sku;

/// Bentuk produk yang dikirim ke frontend. `category_name` hasil JOIN,
/// mengikuti skema `Product` di `contracts/api.yaml`.
///
/// Daftar kolomnya ditulis ulang di tiap query, bukan disatukan lewat
/// konstanta: `query_as!` memeriksa SQL saat compile, jadi yang diterimanya
/// harus berupa literal utuh. Harganya pengulangan; imbalannya, kolom yang
/// salah ketik ketahuan saat build, bukan saat halaman dibuka.
#[derive(Debug, Serialize)]
pub struct Product {
    pub id: Uuid,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    /// Nama identifikasi internal: yang dicari pegawai di kasir dan dibaca
    /// pengepak. Pendek dan cepat dikenali.
    pub name: String,
    /// Judul untuk marketplace. `None` berarti belum diisi -- pemanggil yang
    /// memutuskan apakah jatuh kembali ke `name`.
    pub seo_name: Option<String>,
    /// Selalu hasil rakitan dari merek/jenis/warna/ukuran, tidak pernah
    /// diketik manual. Lihat modul `sku`.
    pub sku: Option<String>,
    pub brand_name: Option<String>,
    pub product_type: Option<String>,
    /// Mutu / kelas ukuran barang, mis. "SP 08", "Super Besar", "B".
    pub variant_grade: Option<String>,
    /// Isi satu pack, mis. "2 kg". Satu SKU berarti satu pack.
    pub variant_size: Option<String>,
    /// Terisi berarti baris ini varian dari produk lain.
    pub parent_id: Option<Uuid>,
    /// Banyaknya varian di bawah produk ini. Induk yang punya varian tidak
    /// dijual langsung -- yang dijual varian-variannya.
    pub variant_count: i64,
    /// Harga dasar: yang dipakai kasir di toko, sekaligus rujukan saat harga
    /// kanal belum diisi.
    pub price: Decimal,
    /// `None` berarti belum diatur -- bukan gratis. Lihat migrasi 0005.
    pub price_shopee: Option<Decimal>,
    /// Mencakup Tokopedia; keduanya satu kanal sejak akuisisi TikTok.
    pub price_tiktok: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub stock_qty: i32,
    pub low_stock_threshold: i32,
    pub image_url: Option<String>,
    /// Kedaluwarsa terdekat dari batch yang MASIH BERSISA. Diambil di query
    /// yang sama supaya daftar produk bisa menandai barang yang mendekati
    /// kedaluwarsa tanpa query tambahan per baris.
    ///
    /// Batch yang sudah habis tidak ikut. Sebelum migrasi 0011 sisa batch
    /// tidak pernah berkurang, jadi tanggal batch yang barangnya sudah lama
    /// terjual tetap menyala merah selamanya -- dan peringatan yang selalu
    /// menyala adalah peringatan yang berhenti dibaca.
    pub nearest_expiry: Option<NaiveDate>,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct StockAdjustment {
    pub id: Uuid,
    pub product_id: Uuid,
    pub change_qty: i32,
    pub reason: String,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub stock_before: i32,
    pub stock_after: i32,
    pub adjusted_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Satu catatan barang masuk. `quantity` adalah jumlah yang MASUK saat itu,
/// bukan sisa yang belum terjual -- sisanya ada di `remaining_qty`.
///
/// Sejak migrasi 0011 `remaining_qty` adalah stok sungguhan:
/// `products.stock_qty` sama dengan jumlah `remaining_qty` seluruh batch
/// produk itu, dan `stock.rs` yang menjaganya. `quantity` tinggal menjadi
/// fakta sejarah tentang isi kiriman.
#[derive(Debug, Serialize)]
pub struct ProductBatch {
    pub id: Uuid,
    pub product_id: Uuid,
    pub batch_number: Option<String>,
    /// Harga beli per batch. Dipakai untuk menghitung laba barang terjual.
    pub purchase_price: Option<Decimal>,
    /// Isi kiriman saat datang. Tidak pernah berubah.
    pub quantity: i32,
    /// Sisa yang belum keluar. Inilah yang dikurangi penjualan.
    pub remaining_qty: i32,
    pub expiry_date: Option<NaiveDate>,
    /// Rak tempat kiriman INI ditaruh, mis. "Rak A3". Sifat kiriman, bukan
    /// sifat barang: dua kiriman produk yang sama bisa tinggal di rak yang
    /// berbeda, dan sejak migrasi 0016 masing-masing menyebutkan sendiri.
    pub storage_location: Option<String>,
    pub received_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
}

/// Bagian katalog yang diminta pemanggil. Halaman produk mengurus induk,
/// kasir hanya boleh melihat yang benar-benar bisa dijual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Semua baris, varian sekalipun.
    Semua,
    /// Hanya produk induk (`parent_id IS NULL`).
    Induk,
    /// Hanya yang bisa dijual: produk tanpa varian, dan varian itu sendiri.
    /// Induk yang punya varian tidak punya harga yang berlaku -- harganya ada
    /// di masing-masing varian -- jadi tidak boleh muncul di kasir.
    Terjual,
}

impl Scope {
    fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Semua => None,
            Self::Induk => Some("induk"),
            Self::Terjual => Some("terjual"),
        }
    }
}

pub struct ProductFilter {
    pub search: Option<String>,
    pub category_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub scope: Scope,
    pub only_active: bool,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_products(
    pool: &PgPool,
    filter: &ProductFilter,
) -> AppResult<(Vec<Product>, i64)> {
    // `$1 IS NULL OR ...` membuat satu query melayani semua kombinasi filter.
    // Alternatifnya merangkai SQL sebagai string, yang menutup pintu bagi
    // pemeriksaan query saat compile.
    let rows = sqlx::query_as!(
        Product,
        r#"
        SELECT
            p.id, p.category_id, c.name AS "category_name?", p.name, p.seo_name, p.sku,
            p.brand_name, p.product_type, p.variant_grade, p.variant_size, p.parent_id,
            (SELECT count(*) FROM products v WHERE v.parent_id = p.id) AS "variant_count!",
            p.price, p.price_shopee, p.price_tiktok, p.cost_price,
            p.stock_qty, p.low_stock_threshold,
            p.image_url,
            (SELECT min(b.expiry_date) FROM product_batches b
             WHERE b.product_id = p.id AND b.remaining_qty > 0)
                AS "nearest_expiry?",
            p.is_active, p.created_by,
            p.created_at AS "created_at!", p.updated_at AS "updated_at!"
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE ($1::text IS NULL
               OR p.name ILIKE '%' || $1 || '%'
               OR p.seo_name ILIKE '%' || $1 || '%'
               OR p.sku ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR p.category_id = $2)
          AND (NOT $3::bool OR p.is_active)
          AND ($4::uuid IS NULL OR p.parent_id = $4)
          AND ($5::text IS NULL
               OR ($5 = 'induk' AND p.parent_id IS NULL)
               OR ($5 = 'terjual'
                   AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)))
        -- `p.id` bukan hiasan: nama produk boleh kembar, dan tanpa pemutus
        -- yang pasti urutan dua baris bernama sama bisa bertukar antar kueri.
        -- Di daftar berhalaman itu berarti satu produk muncul dua kali dan
        -- produk lain tidak pernah muncul sama sekali.
        ORDER BY p.name, p.id
        LIMIT $6 OFFSET $7
        "#,
        filter.search.as_deref(),
        filter.category_id,
        filter.only_active,
        filter.parent_id,
        filter.scope.as_str(),
        filter.limit,
        filter.offset
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM products p
        WHERE ($1::text IS NULL
               OR p.name ILIKE '%' || $1 || '%'
               OR p.seo_name ILIKE '%' || $1 || '%'
               OR p.sku ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR p.category_id = $2)
          AND (NOT $3::bool OR p.is_active)
          AND ($4::uuid IS NULL OR p.parent_id = $4)
          AND ($5::text IS NULL
               OR ($5 = 'induk' AND p.parent_id IS NULL)
               OR ($5 = 'terjual'
                   AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)))
        "#,
        filter.search.as_deref(),
        filter.category_id,
        filter.only_active,
        filter.parent_id,
        filter.scope.as_str()
    )
    .fetch_one(pool)
    .await?;

    Ok((rows, total))
}

pub async fn find_product(pool: &PgPool, id: Uuid) -> AppResult<Option<Product>> {
    let row = sqlx::query_as!(
        Product,
        r#"
        SELECT
            p.id, p.category_id, c.name AS "category_name?", p.name, p.seo_name, p.sku,
            p.brand_name, p.product_type, p.variant_grade, p.variant_size, p.parent_id,
            (SELECT count(*) FROM products v WHERE v.parent_id = p.id) AS "variant_count!",
            p.price, p.price_shopee, p.price_tiktok, p.cost_price,
            p.stock_qty, p.low_stock_threshold,
            p.image_url,
            (SELECT min(b.expiry_date) FROM product_batches b
             WHERE b.product_id = p.id AND b.remaining_qty > 0)
                AS "nearest_expiry?",
            p.is_active, p.created_by,
            p.created_at AS "created_at!", p.updated_at AS "updated_at!"
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE p.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Varian sebuah produk, diurutkan menurut sumbu variannya supaya daftarnya
/// tampil dengan urutan yang sama setiap kali dibuka.
pub async fn list_variants(pool: &PgPool, parent_id: Uuid) -> AppResult<Vec<Product>> {
    let rows = sqlx::query_as!(
        Product,
        r#"
        SELECT
            p.id, p.category_id, c.name AS "category_name?", p.name, p.seo_name, p.sku,
            p.brand_name, p.product_type, p.variant_grade, p.variant_size, p.parent_id,
            (SELECT count(*) FROM products v WHERE v.parent_id = p.id) AS "variant_count!",
            p.price, p.price_shopee, p.price_tiktok, p.cost_price,
            p.stock_qty, p.low_stock_threshold,
            p.image_url,
            (SELECT min(b.expiry_date) FROM product_batches b
             WHERE b.product_id = p.id AND b.remaining_qty > 0)
                AS "nearest_expiry?",
            p.is_active, p.created_by,
            p.created_at AS "created_at!", p.updated_at AS "updated_at!"
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE p.parent_id = $1
        ORDER BY p.variant_grade NULLS FIRST, p.variant_size NULLS FIRST, p.name
        "#,
        parent_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub struct NewProduct {
    pub name: String,
    pub seo_name: Option<String>,
    pub sku: Option<String>,
    pub brand_name: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<String>,
    pub parent_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub price: Decimal,
    pub price_shopee: Option<Decimal>,
    pub price_tiktok: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub low_stock_threshold: i32,
    pub image_url: Option<String>,
    pub created_by: Uuid,
}

/// Produk selalu lahir dengan stok nol. Stok awal masuk lewat batch (lihat
/// `insert_batch` dan `stock::tambah`), supaya tidak pernah ada butir stok
/// yang muncul tanpa baris ledger yang menjelaskan asalnya.
pub async fn insert_product(
    tx: &mut Transaction<'_, Postgres>,
    input: &NewProduct,
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO products
            (name, seo_name, sku, brand_name, product_type, variant_grade, variant_size,
             parent_id, category_id, price, price_shopee, price_tiktok,
             cost_price, stock_qty, low_stock_threshold, image_url, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 0,
                $14, $15, $16)
        RETURNING id
        "#,
        input.name,
        input.seo_name,
        input.sku,
        input.brand_name,
        input.product_type,
        input.variant_grade,
        input.variant_size,
        input.parent_id,
        input.category_id,
        input.price,
        input.price_shopee,
        input.price_tiktok,
        input.cost_price,
        input.low_stock_threshold,
        input.image_url,
        input.created_by
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

/// Kolom yang boleh dikosongkan kembali, bukan sekadar diganti isinya.
///
/// `None` berarti kolomnya tidak disebut permintaan -- biarkan apa adanya.
/// `Some(None)` berarti pengguna sengaja mengosongkannya. Keduanya harus
/// dibedakan: tanpa itu, harga Shopee yang terlanjur diisi tidak akan pernah
/// bisa dikembalikan ke "belum diatur", dan warna varian yang salah ketik
/// menempel selamanya di SKU.
pub type Ubah<T> = Option<Option<T>>;

/// Deserializer untuk `Ubah<T>`, wajib dipasang lewat `deserialize_with`.
///
/// Tanpa ini serde membaca `"price_shopee": null` sebagai `None` -- sama
/// dengan field yang tidak disebut sama sekali -- sehingga permintaan
/// mengosongkan kolom diam-diam berubah jadi "biarkan apa adanya". Di sini
/// yang null dibungkus jadi `Some(None)`, dan yang benar-benar tidak disebut
/// ditangani `#[serde(default)]` di field-nya.
pub fn ubah_terkirim<'de, D, T>(deserializer: D) -> Result<Ubah<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

fn teks(kolom: &Ubah<String>) -> (bool, Option<&str>) {
    match kolom {
        None => (false, None),
        Some(isi) => (true, isi.as_deref()),
    }
}

fn salinan<T: Copy>(kolom: &Ubah<T>) -> (bool, Option<T>) {
    match kolom {
        None => (false, None),
        Some(isi) => (true, *isi),
    }
}

/// Semua kolom opsional: yang tidak disebut dibiarkan seperti semula. Stok
/// TIDAK ikut di sini -- satu-satunya jalan mengubah stok adalah lewat
/// `stock.rs`, supaya tidak ada perubahan yang lolos tanpa tercatat di
/// ledger.
#[derive(Default)]
pub struct ProductPatch {
    pub name: Option<String>,
    pub seo_name: Ubah<String>,
    /// Selalu hasil rakitan ulang di `routes`, tidak pernah dari pengguna.
    pub sku: Ubah<String>,
    pub brand_name: Ubah<String>,
    pub product_type: Ubah<String>,
    pub variant_grade: Ubah<String>,
    pub variant_size: Ubah<String>,
    pub category_id: Ubah<Uuid>,
    pub price: Option<Decimal>,
    pub price_shopee: Ubah<Decimal>,
    pub price_tiktok: Ubah<Decimal>,
    pub cost_price: Ubah<Decimal>,
    pub low_stock_threshold: Option<i32>,
    pub image_url: Ubah<String>,
    pub is_active: Option<bool>,
}

pub async fn update_product(pool: &PgPool, id: Uuid, patch: &ProductPatch) -> AppResult<bool> {
    let (ubah_seo, seo_name) = teks(&patch.seo_name);
    let (ubah_sku, sku) = teks(&patch.sku);
    let (ubah_merek, brand_name) = teks(&patch.brand_name);
    let (ubah_jenis, product_type) = teks(&patch.product_type);
    let (ubah_warna, variant_grade) = teks(&patch.variant_grade);
    let (ubah_ukuran, variant_size) = teks(&patch.variant_size);
    let (ubah_kategori, category_id) = salinan(&patch.category_id);
    let (ubah_shopee, price_shopee) = salinan(&patch.price_shopee);
    let (ubah_tiktok, price_tiktok) = salinan(&patch.price_tiktok);
    let (ubah_modal, cost_price) = salinan(&patch.cost_price);
    let (ubah_gambar, image_url) = teks(&patch.image_url);

    // Kolom yang tidak boleh NULL memakai COALESCE; sisanya memakai CASE
    // dengan penanda tersendiri, karena COALESCE tidak bisa membedakan
    // "tidak disebut" dari "sengaja dikosongkan".
    let hasil = sqlx::query!(
        r#"
        UPDATE products SET
            name                = COALESCE($2, name),
            price               = COALESCE($3, price),
            low_stock_threshold = COALESCE($4, low_stock_threshold),
            is_active           = COALESCE($5, is_active),
            seo_name            = CASE WHEN $6::bool  THEN $7::varchar  ELSE seo_name END,
            sku                 = CASE WHEN $8::bool  THEN $9::varchar  ELSE sku END,
            brand_name          = CASE WHEN $10::bool THEN $11::varchar ELSE brand_name END,
            product_type        = CASE WHEN $12::bool THEN $13::varchar ELSE product_type END,
            variant_grade       = CASE WHEN $14::bool THEN $15::varchar ELSE variant_grade END,
            variant_size        = CASE WHEN $16::bool THEN $17::varchar ELSE variant_size END,
            category_id         = CASE WHEN $18::bool THEN $19::uuid    ELSE category_id END,
            price_shopee        = CASE WHEN $20::bool THEN $21::numeric ELSE price_shopee END,
            price_tiktok        = CASE WHEN $22::bool THEN $23::numeric ELSE price_tiktok END,
            cost_price          = CASE WHEN $24::bool THEN $25::numeric ELSE cost_price END,
            image_url           = CASE WHEN $26::bool THEN $27::varchar ELSE image_url END,
            updated_at          = now()
        WHERE id = $1
        "#,
        id,
        patch.name.as_deref(),
        patch.price,
        patch.low_stock_threshold,
        patch.is_active,
        ubah_seo,
        seo_name,
        ubah_sku,
        sku,
        ubah_merek,
        brand_name,
        ubah_jenis,
        product_type,
        ubah_warna,
        variant_grade,
        ubah_ukuran,
        variant_size,
        ubah_kategori,
        category_id,
        ubah_shopee,
        price_shopee,
        ubah_tiktok,
        price_tiktok,
        ubah_modal,
        cost_price,
        ubah_gambar,
        image_url
    )
    .execute(pool)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

/// Memastikan sebuah SKU belum dipakai produk lain.
///
/// Tidak ada akhiran pembeda otomatis. Dua produk dengan jenis, grade,
/// merek, dan ukuran yang sama persis memang menghasilkan SKU yang sama, dan
/// yang kedua ditolak di sini: `CBSB-AFC-2KG-2` tidak memberi tahu siapa pun
/// apa bedanya dari `CBSB-AFC-2KG`, selain melewati batas 12 karakter. Kalau
/// dua barang memang berbeda, yang membedakannya harus ada di atributnya.
pub async fn sku_harus_bebas(pool: &PgPool, sku: &str, kecuali: Option<Uuid>) -> AppResult<()> {
    if sku_dipakai(pool, sku, kecuali).await? {
        return Err(AppError::conflict(format!(
            "SKU \"{sku}\" sudah dipakai produk lain. \
             Bedakan jenis produk, grade, merek, atau ukurannya."
        )));
    }

    Ok(())
}

pub async fn sku_dipakai(pool: &PgPool, sku: &str, kecuali: Option<Uuid>) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM products WHERE sku = $1 AND ($2::uuid IS NULL OR id <> $2)) AS "ada!""#,
        sku,
        kecuali
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

pub async fn category_ada(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM categories WHERE id = $1) AS "ada!""#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

/// Apa yang menahan sebuah produk sehingga tidak boleh dihapus. `None`
/// berarti tidak ada -- produk itu belum menyentuh apa pun dan aman dibuang.
pub async fn penahan_hapus(pool: &PgPool, id: Uuid) -> AppResult<Option<&'static str>> {
    let row = sqlx::query!(
        r#"
        SELECT
            EXISTS(SELECT 1 FROM products WHERE parent_id = $1)              AS "varian!",
            EXISTS(SELECT 1 FROM transaction_items WHERE product_id = $1)    AS "penjualan!",
            EXISTS(SELECT 1 FROM ticket_items WHERE product_id = $1)         AS "tiket!",
            EXISTS(SELECT 1 FROM external_order_items WHERE product_id = $1) AS "pesanan!",
            EXISTS(SELECT 1 FROM channel_listings WHERE product_id = $1)     AS "listing!",
            EXISTS(SELECT 1 FROM shopping_list_items WHERE product_id = $1)  AS "belanja!"
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(if row.varian {
        Some("masih punya varian")
    } else if row.penjualan {
        Some("sudah pernah terjual di kasir")
    } else if row.tiket {
        Some("tercatat di tiket packing")
    } else if row.pesanan {
        Some("tercatat di pesanan marketplace")
    } else if row.listing {
        Some("terhubung ke listing marketplace")
    } else if row.belanja {
        Some("ada di daftar belanja")
    } else {
        None
    })
}

/// Menghapus produk berikut catatan yang hanya berarti bersama produk itu:
/// batch barang masuk dan baris ledger stoknya. Pemanggil wajib memeriksa
/// `penahan_hapus` lebih dulu -- riwayat penjualan tidak pernah ikut terhapus
/// lewat jalan ini.
pub async fn delete_product(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let mut tx = pool.begin().await?;

    sqlx::query!("DELETE FROM product_batches WHERE product_id = $1", id)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM stock_adjustments WHERE product_id = $1", id)
        .execute(&mut *tx)
        .await?;

    let hasil = sqlx::query!("DELETE FROM products WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(hasil.rows_affected() > 0)
}

// ---------------------------------------------------------------------
// Batch barang masuk
// ---------------------------------------------------------------------

pub async fn list_batches(pool: &PgPool, product_id: Uuid) -> AppResult<Vec<ProductBatch>> {
    let rows = sqlx::query_as!(
        ProductBatch,
        r#"
        SELECT id, product_id, batch_number, purchase_price, quantity, remaining_qty,
               expiry_date, storage_location, received_at AS "received_at!", created_by
        FROM product_batches
        WHERE product_id = $1
        ORDER BY expiry_date NULLS LAST, received_at DESC
        "#,
        product_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub struct NewBatch {
    pub product_id: Uuid,
    pub batch_number: Option<String>,
    pub purchase_price: Option<Decimal>,
    pub quantity: i32,
    pub expiry_date: Option<NaiveDate>,
    pub storage_location: Option<String>,
    pub created_by: Uuid,
}

/// Mencatat satu batch. Penambahan stoknya dikerjakan pemanggil lewat
/// `stock.rs` di transaksi yang sama, jadi batch dan ledger tidak pernah bisa
/// bercerita berbeda.
///
/// `remaining_qty` sengaja MULAI DARI NOL, bukan dari `quantity`. Kolom itu
/// adalah stok sungguhan, dan satu-satunya yang boleh menggerakkannya adalah
/// `stock.rs` -- kalau di sini pun ikut mengisinya, ada dua penulis untuk
/// satu angka dan invarian stok = jumlah sisa batch kehilangan penjaganya.
pub async fn insert_batch(tx: &mut Transaction<'_, Postgres>, input: &NewBatch) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO product_batches
            (product_id, batch_number, purchase_price, quantity, remaining_qty, expiry_date,
             storage_location, created_by)
        VALUES ($1, $2, $3, $4, 0, $5, $6, $7)
        RETURNING id
        "#,
        input.product_id,
        input.batch_number,
        input.purchase_price,
        input.quantity,
        input.expiry_date,
        input.storage_location,
        input.created_by
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

/// Seluruh batch yang masih bersisa, untuk produk yang benar-benar bisa
/// dijual. Dibaca kasir sekali per halaman supaya bisa memilih sendiri batch
/// mana yang dikeluarkan, tanpa satu permintaan per produk.
///
/// Urutannya SAMA PERSIS dengan urutan FEFO di `stock.rs`, jadi pilihan
/// teratas di layar adalah pilihan yang akan diambil otomatis kalau kasir
/// tidak memilih apa-apa. Dua urutan yang berbeda untuk hal yang sama adalah
/// cara paling halus membuat kasir tidak percaya pada layarnya.
pub async fn list_batches_tersedia(pool: &PgPool) -> AppResult<Vec<ProductBatch>> {
    let rows = sqlx::query_as!(
        ProductBatch,
        r#"
        SELECT b.id, b.product_id, b.batch_number, b.purchase_price, b.quantity, b.remaining_qty,
               b.expiry_date, b.storage_location, b.received_at AS "received_at!", b.created_by
        FROM product_batches b
        JOIN products p ON p.id = b.product_id
        WHERE b.remaining_qty > 0
          AND p.is_active
          AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)
        ORDER BY b.product_id, b.expiry_date NULLS LAST, b.received_at, b.id
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Dibaca di dalam transaksi penghapusan: jumlahnya dipakai untuk menarik
/// kembali stok yang dulu ditambahkan batch ini.
pub async fn find_batch(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    id: Uuid,
) -> AppResult<Option<ProductBatch>> {
    let row = sqlx::query_as!(
        ProductBatch,
        r#"
        SELECT id, product_id, batch_number, purchase_price, quantity, remaining_qty,
               expiry_date, storage_location, received_at AS "received_at!", created_by
        FROM product_batches
        WHERE id = $1 AND product_id = $2
        "#,
        id,
        product_id
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row)
}

/// Kolom batch yang boleh dikoreksi sesudah kirimannya dicatat.
///
/// Semuanya `Ubah`: tidak disebut berarti biarkan, `null` berarti kosongkan.
/// Ketiganya harus bisa dikembalikan ke kosong -- harga beli yang salah
/// ketik, tanggal kedaluwarsa yang ternyata tidak ada di kemasan, dan rak
/// yang barangnya sudah dipindah entah ke mana.
///
/// `quantity` TIDAK ada di sini dan tidak akan pernah ada. Ia menggerakkan
/// stok, dan satu-satunya jalan stok berubah adalah lewat `stock.rs` supaya
/// tiap butir punya baris ledger yang menjelaskan asalnya. Batch yang salah
/// jumlahnya dibatalkan lalu dicatat ulang.
#[derive(Default)]
pub struct BatchPatch {
    pub purchase_price: Ubah<Decimal>,
    pub expiry_date: Ubah<NaiveDate>,
    pub storage_location: Ubah<String>,
}

/// Mengoreksi batch yang sudah tercatat.
///
/// HARGA BELI. Batch yang sudah ada sejak sebelum migrasi 0014 tidak punya
/// harga beli sama sekali, dan tanpa jalan ini laporan laba akan selamanya
/// menghitungnya nol.
///
/// KEDALUWARSA. Tanggal yang salah ketik bukan cuma angka yang jelek di
/// layar: ia menentukan urutan FEFO dan menyalakan peringatan merah di
/// daftar produk. Mengoreksinya di sini aman karena tidak ada butir stok
/// yang berpindah -- yang berubah hanya urutan keluarnya batch berikutnya.
///
/// LOKASI. Rak tempat kiriman ini ditaruh, dibaca pengepak. Barang memang
/// dipindah antar rak, jadi kolom yang tidak bisa dikoreksi akan menjadi
/// petunjuk yang salah dalam hitungan minggu.
pub async fn update_batch(
    pool: &PgPool,
    product_id: Uuid,
    id: Uuid,
    patch: &BatchPatch,
) -> AppResult<bool> {
    let (ubah_harga, purchase_price) = salinan(&patch.purchase_price);
    let (ubah_exp, expiry_date) = salinan(&patch.expiry_date);
    let (ubah_lokasi, storage_location) = teks(&patch.storage_location);

    let hasil = sqlx::query!(
        r#"
        UPDATE product_batches SET
            purchase_price   = CASE WHEN $3::bool THEN $4::numeric ELSE purchase_price END,
            expiry_date      = CASE WHEN $5::bool THEN $6::date    ELSE expiry_date END,
            storage_location = CASE WHEN $7::bool THEN $8::varchar ELSE storage_location END
        WHERE id = $1 AND product_id = $2
        "#,
        id,
        product_id,
        ubah_harga,
        purchase_price,
        ubah_exp,
        expiry_date,
        ubah_lokasi,
        storage_location
    )
    .execute(pool)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

pub async fn delete_batch(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<()> {
    sqlx::query!("DELETE FROM product_batches WHERE id = $1", id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------
// Kategori dan ledger stok
// ---------------------------------------------------------------------

pub async fn list_categories(pool: &PgPool) -> AppResult<Vec<Category>> {
    let rows = sqlx::query_as!(
        Category,
        r#"SELECT id, name, created_by, created_at AS "created_at!" FROM categories ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn insert_category(pool: &PgPool, name: &str, created_by: Uuid) -> AppResult<Category> {
    let row = sqlx::query_as!(
        Category,
        r#"
        INSERT INTO categories (name, created_by)
        VALUES ($1, $2)
        RETURNING id, name, created_by, created_at AS "created_at!"
        "#,
        name,
        created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn category_nama_dipakai(pool: &PgPool, name: &str) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM categories WHERE lower(name) = lower($1)) AS "ada!""#,
        name
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

pub async fn list_stock_adjustments(
    pool: &PgPool,
    product_id: Uuid,
    limit: i64,
) -> AppResult<Vec<StockAdjustment>> {
    let rows = sqlx::query_as!(
        StockAdjustment,
        r#"
        SELECT id, product_id, change_qty, reason, reference_type, reference_id,
               stock_before, stock_after, adjusted_by_user_id,
               created_at AS "created_at!"
        FROM stock_adjustments
        WHERE product_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT $2
        "#,
        product_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// ---------------------------------------------------------------------
// Kamus kode SKU
// ---------------------------------------------------------------------

/// Satu entri kamus seperti yang dilihat frontend.
#[derive(Debug, Serialize)]
pub struct SkuCode {
    pub id: Uuid,
    /// `jenis`, `grade`, `merek`, atau `ukuran`.
    pub kind: String,
    /// Nilai atribut apa adanya, mis. "SP 08".
    pub source: String,
    pub code: String,
}

/// Seluruh isi kamus, dirakit jadi bentuk yang dipakai `catalog::sku`.
///
/// Dibaca sekali per perakitan SKU, bukan satu query per bagian: isinya
/// puluhan baris dan perakitan SKU cuma terjadi saat produk dibuat.
///
/// Baris ber-`kind` di luar daftar yang dikenal dilewati, bukan membuat
/// seluruh perakitan gagal. CHECK constraint di database sudah menjaganya,
/// jadi kalau sampai ada, itu bug -- dan menolak membuat produk karena satu
/// baris kamus yang rusak menghentikan pekerjaan yang tidak ada hubungannya.
pub async fn kamus_sku(pool: &PgPool) -> AppResult<sku::Kamus> {
    let rows = sqlx::query!(r#"SELECT kind, source, code FROM sku_codes"#)
        .fetch_all(pool)
        .await?;

    let entri = rows.into_iter().filter_map(|r| {
        let bagian = sku::Bagian::parse(&r.kind).or_else(|| {
            tracing::error!(kind = %r.kind, "bagian kamus SKU tidak dikenal");
            None
        })?;
        Some((bagian, r.source, r.code))
    });

    Ok(sku::Kamus::baru(entri))
}

pub async fn list_sku_codes(pool: &PgPool) -> AppResult<Vec<SkuCode>> {
    let rows = sqlx::query_as!(
        SkuCode,
        r#"
        SELECT id, kind, source, code
        FROM sku_codes
        ORDER BY kind, source
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// `source_key` ditulis di sini, tidak pernah diterima dari pemanggil: ia
/// turunan dari `source` lewat aturan yang sama dengan yang dipakai saat
/// mencari (`sku::kunci`). Dua tempat yang menghitungnya sendiri-sendiri akan
/// membuat entri yang tersimpan tidak pernah ditemukan.
pub async fn insert_sku_code(
    pool: &PgPool,
    kind: sku::Bagian,
    source: &str,
    code: &str,
    created_by: Uuid,
) -> AppResult<SkuCode> {
    let source_key = sku::kunci(source);

    let row = sqlx::query_as!(
        SkuCode,
        r#"
        INSERT INTO sku_codes (kind, source, source_key, code, created_by)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (kind, source_key) DO UPDATE
            SET source = EXCLUDED.source,
                code = EXCLUDED.code,
                updated_at = CURRENT_TIMESTAMP
        RETURNING id, kind, source, code
        "#,
        kind.as_str(),
        source.trim(),
        source_key,
        code,
        created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn delete_sku_code(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let hasil = sqlx::query!(r#"DELETE FROM sku_codes WHERE id = $1"#, id)
        .execute(pool)
        .await?;

    Ok(hasil.rows_affected() > 0)
}
