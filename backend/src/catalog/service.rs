//! Logika pembuatan/penyuntingan produk, dipakai bersama HTTP handler
//! (`routes.rs`) dan proses commit impor massal (`crate::import`).
//!
//! Dipisah dari `routes.rs` supaya jalur commit impor menulis produk lewat
//! aturan yang SAMA PERSIS dengan form manual -- rakitan SKU, validasi
//! harga/kategori, dan pencatatan batch awal lewat ledger stok -- bukan
//! INSERT langsung yang mudah lupa satu dari semua itu.

use super::repo::{self, NewBatch, NewProduct, Product};
use super::sku;
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use chrono::NaiveDate;
use rust_decimal::Decimal;

// `ProductPatch` didefinisikan di `repo` (privat ke modul `catalog`), tapi
// pemanggil di luar `catalog` (proses commit impor massal) perlu
// menyusunnya sendiri untuk memanggil `update_product` di bawah -- jadi
// diekspor ulang lewat sini, satu-satunya pintu keluar `catalog::service`.
pub(crate) use super::repo::ProductPatch;
use uuid::Uuid;

pub struct CreateProductInput {
    pub name: String,
    pub seo_name: Option<String>,
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
    pub is_active: bool,
    pub low_stock_threshold: Option<i32>,
    pub image_url: Option<String>,
    pub stock_qty: i32,
    pub purchase_price: Option<Decimal>,
    pub batch_number: Option<String>,
    pub expiry_date: Option<NaiveDate>,
    pub storage_location: Option<String>,
}

/// Membuat produk baru: rakit SKU, simpan produk, dan -- kalau ada stok
/// awal -- catat batch pertamanya, semua dalam satu transaksi. Dipakai
/// `routes::create_product` (form manual) dan proses commit impor massal.
pub(crate) async fn create_product(
    state: &AppState,
    created_by: Uuid,
    input: CreateProductInput,
) -> AppResult<Uuid> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("Nama produk wajib diisi."));
    }
    periksa_harga(input.price, input.price_shopee, input.price_tiktok)?;
    if input.stock_qty < 0 {
        return Err(AppError::bad_request("Stok awal tidak boleh negatif."));
    }

    // Varian menempel pada induk yang harus benar-benar ada, dan induk itu
    // tidak boleh varian: katalog bertingkat-tingkat tidak punya wujud yang
    // masuk akal di layar kasir maupun di marketplace.
    if let Some(parent_id) = input.parent_id {
        let induk = ambil_produk(state, parent_id).await?;
        if induk.parent_id.is_some() {
            return Err(AppError::bad_request(
                "Varian tidak bisa punya varian lagi.",
            ));
        }
    }

    if let Some(category_id) = input.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    let brand_name = bersihkan(input.brand_name);
    let product_type = bersihkan(input.product_type);
    let variant_grade = bersihkan(input.variant_grade);
    let variant_size = bersihkan(input.variant_size);

    let sku = rakit_sku(
        state,
        &brand_name,
        &product_type,
        &variant_grade,
        &variant_size,
    )
    .await?;

    let batch_number = bersihkan(input.batch_number);

    // Produk, batch pertamanya, dan baris ledger stok ditulis dalam satu
    // transaksi: produk yang tersimpan tanpa stok awalnya akan tampil habis
    // padahal barangnya ada di rak.
    let mut tx = state.pool.begin().await?;

    let id = repo::insert_product(
        &mut tx,
        &NewProduct {
            name: name.to_string(),
            seo_name: bersihkan(input.seo_name),
            sku: Some(sku),
            brand_name,
            product_type,
            variant_grade,
            variant_size,
            parent_id: input.parent_id,
            category_id: input.category_id,
            price: input.price,
            price_shopee: input.price_shopee,
            price_tiktok: input.price_tiktok,
            cost_price: input.cost_price,
            is_active: input.is_active,
            low_stock_threshold: input.low_stock_threshold.unwrap_or(5),
            image_url: bersihkan(input.image_url),
            created_by,
        },
    )
    .await?;

    if input.stock_qty > 0 {
        catat_batch(
            &mut tx,
            &NewBatch {
                product_id: id,
                batch_number,
                purchase_price: input.purchase_price,
                quantity: input.stock_qty,
                expiry_date: input.expiry_date,
                storage_location: bersihkan(input.storage_location),
                created_by,
            },
        )
        .await?;
    }

    tx.commit().await?;
    Ok(id)
}

/// Menyunting produk yang sudah ada. `patch` sudah harus lengkap (termasuk
/// koreksi SKU eksplisit kalau ada -- lihat `routes::koreksi_sku`, yang
/// TIDAK dipanggil dari sini karena impor massal tidak pernah menyentuh SKU
/// produk yang sudah ada).
pub(crate) async fn update_product(
    state: &AppState,
    id: Uuid,
    patch: ProductPatch,
) -> AppResult<bool> {
    let sekarang = ambil_produk(state, id).await?;

    periksa_harga(
        patch.price.unwrap_or(sekarang.price),
        patch.price_shopee.flatten(),
        patch.price_tiktok.flatten(),
    )?;

    if let Some(Some(category_id)) = patch.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    repo::update_product(&state.pool, id, &patch).await
}

pub(crate) async fn ambil_produk(state: &AppState, id: Uuid) -> AppResult<Product> {
    repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))
}

/// Harga diperiksa di sini supaya pesannya menyebut marketplace mana yang
/// salah. CHECK constraint di database tetap ada sebagai jaring terakhir,
/// tapi galatnya tidak bisa menyebutkan itu.
pub(crate) fn periksa_harga(
    price: Decimal,
    price_shopee: Option<Decimal>,
    price_tiktok: Option<Decimal>,
) -> AppResult<()> {
    if price.is_sign_negative() {
        return Err(AppError::bad_request("Harga tidak boleh negatif."));
    }
    if price_shopee.is_some_and(|h| h.is_sign_negative()) {
        return Err(AppError::bad_request("Harga Shopee tidak boleh negatif."));
    }
    if price_tiktok.is_some_and(|h| h.is_sign_negative()) {
        return Err(AppError::bad_request(
            "Harga Tokopedia/TikTok Shop tidak boleh negatif.",
        ));
    }
    Ok(())
}

/// SKU untuk produk yang baru dibuat, dipastikan belum dipakai produk lain.
/// Dipanggil sekali seumur produk: menyunting atribut tidak merakit ulang
/// SKU-nya (lihat dokumentasi modul `sku`).
pub(crate) async fn rakit_sku(
    state: &AppState,
    brand_name: &Option<String>,
    product_type: &Option<String>,
    variant_grade: &Option<String>,
    variant_size: &Option<String>,
) -> AppResult<String> {
    let kamus = repo::kamus_sku(&state.pool).await?;

    let kode = sku::rakit(
        &kamus,
        product_type.as_deref(),
        variant_grade.as_deref(),
        brand_name.as_deref(),
        variant_size.as_deref(),
    )
    .map_err(|err| AppError::bad_request(err.to_string()))?;

    repo::sku_harus_bebas(&state.pool, &kode, None).await?;
    Ok(kode)
}

/// Menulis satu batch sekaligus menambah stoknya lewat ledger. Keduanya
/// selalu terjadi bersama, jadi disatukan di sini supaya tidak ada pemanggil
/// yang lupa salah satunya.
pub(crate) async fn catat_batch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &NewBatch,
) -> AppResult<Uuid> {
    // Batch lahir dengan sisa nol; `stock::tambah` yang menaikkannya ke
    // `quantity`. Dengan begitu `remaining_qty` hanya punya satu penulis,
    // dan baris ledger barang masuk menyebut batch mana yang datang.
    let id = repo::insert_batch(tx, input).await?;

    stock::tambah(
        tx,
        &[StockLine {
            product_id: input.product_id,
            qty: input.quantity,
            batch_id: Some(id),
        }],
        StockReason::Restock,
        input.product_id,
        Some(input.created_by),
    )
    .await?;

    Ok(id)
}

/// Teks opsional dari form: spasi di tepi dibuang, dan yang tersisa kosong
/// diperlakukan sebagai tidak diisi. Tanpa ini, field yang dikosongkan
/// pengguna tersimpan sebagai string kosong dan tampil sebagai nilai yang
/// "ada" tapi tidak menunjukkan apa pun.
pub(crate) fn bersihkan(nilai: Option<String>) -> Option<String> {
    nilai
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
