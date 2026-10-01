//! Logika buat/sunting produk dipakai bersama handler HTTP dan commit impor massal, agar impor memakai aturan SAMA (SKU, validasi harga/kategori, batch awal via ledger), bukan INSERT langsung.

use super::repo::{self, NewBatch, NewProduct, Product};
use super::sku;
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use chrono::NaiveDate;
use rust_decimal::Decimal;

// `ProductPatch` didefinisikan di `repo` (privat ke `catalog`) dan diekspor ulang di sini untuk impor massal, satu-satunya pintu keluar `catalog::service`.
pub(crate) use super::repo::ProductPatch;
use uuid::Uuid;

pub struct CreateProductInput {
    pub name: String,
    pub seo_name: Option<String>,
    pub brand_name: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<Decimal>,
    pub parent_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub price: Decimal,
    pub price_wholesale: Option<Decimal>,
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

/// Membuat produk: rakit SKU, simpan, dan catat batch awal bila ada stok, semua satu transaksi (dipakai `routes::create_product` dan commit impor).
pub(crate) async fn create_product(
    state: &AppState,
    created_by: Uuid,
    input: CreateProductInput,
) -> AppResult<Uuid> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("Nama produk wajib diisi."));
    }
    periksa_harga(
        input.price,
        input.price_wholesale,
        input.price_shopee,
        input.price_tiktok,
    )?;
    periksa_ukuran(input.variant_size)?;
    if input.stock_qty < 0 {
        return Err(AppError::bad_request("Stok awal tidak boleh negatif."));
    }

    // Varian menempel pada induk yang harus ada dan bukan varian, karena katalog bertingkat tak masuk akal di kasir maupun marketplace.
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
    let variant_size = input.variant_size;

    let sku = rakit_sku(
        state,
        &brand_name,
        &product_type,
        &variant_grade,
        variant_size,
    )
    .await?;

    let batch_number = bersihkan(input.batch_number);

    // Produk, batch pertama, dan baris ledger satu transaksi: produk tanpa stok awal akan tampil habis padahal barangnya ada.
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
            price_wholesale: input.price_wholesale,
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

/// Menyunting produk; `patch` harus lengkap, dan `routes::koreksi_sku` tak dipanggil dari sini karena impor tak pernah menyentuh SKU produk yang ada.
pub(crate) async fn update_product(
    state: &AppState,
    id: Uuid,
    patch: ProductPatch,
) -> AppResult<bool> {
    let sekarang = ambil_produk(state, id).await?;

    periksa_harga(
        patch.price.unwrap_or(sekarang.price),
        patch.price_wholesale.unwrap_or(sekarang.price_wholesale),
        patch.price_shopee.flatten(),
        patch.price_tiktok.flatten(),
    )?;
    periksa_ukuran(patch.variant_size.flatten())?;

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

/// Harga diperiksa di sini agar pesannya menyebut marketplace mana yang salah; CHECK di database tetap jaring terakhir.
pub(crate) fn periksa_harga(
    price: Decimal,
    price_wholesale: Option<Decimal>,
    price_shopee: Option<Decimal>,
    price_tiktok: Option<Decimal>,
) -> AppResult<()> {
    if price.is_sign_negative() {
        return Err(AppError::bad_request("Harga ecer tidak boleh negatif."));
    }
    if price_wholesale.is_some_and(|h| h.is_sign_negative()) {
        return Err(AppError::bad_request("Harga grosir tidak boleh negatif."));
    }
    // Grosir yang lebih mahal dari ecer hampir pasti tertukar ketik, dan akibatnya pembeli besar membayar lebih.
    if price_wholesale.is_some_and(|h| h > price) {
        return Err(AppError::bad_request(
            "Harga grosir tidak boleh lebih tinggi dari harga ecer.",
        ));
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

/// Ukuran harus muat di NUMERIC(8,3) dan positif; di sini agar pesannya terbaca, CHECK di database tetap jaring terakhir.
pub(crate) fn periksa_ukuran(ukuran: Option<Decimal>) -> AppResult<()> {
    if ukuran.is_some_and(|u| u <= Decimal::ZERO || u >= Decimal::from(100_000)) {
        return Err(AppError::bad_request(
            "Ukuran harus lebih dari 0 dan kurang dari 100.000 kg.",
        ));
    }
    Ok(())
}

/// SKU produk baru dipastikan belum dipakai, dipanggil sekali seumur produk (atribut yang disunting tak merakit ulang SKU, lihat modul `sku`).
pub(crate) async fn rakit_sku(
    state: &AppState,
    brand_name: &Option<String>,
    product_type: &Option<String>,
    variant_grade: &Option<String>,
    variant_size: Option<Decimal>,
) -> AppResult<String> {
    let kamus = repo::kamus_sku(&state.pool).await?;

    // Ditulis "2 kg" supaya kodenya tetap `2KG` seperti SKU yang sudah beredar; `normalize` membuang nol di belakang koma (2.000 jadi 2).
    let ukuran = variant_size.map(|u| format!("{} kg", u.normalize()));

    let kode = sku::rakit(
        &kamus,
        product_type.as_deref(),
        variant_grade.as_deref(),
        brand_name.as_deref(),
        ukuran.as_deref(),
    )
    .map_err(|err| AppError::bad_request(err.to_string()))?;

    repo::sku_harus_bebas(&state.pool, &kode, None).await?;
    Ok(kode)
}

/// Menulis batch sekaligus menambah stok lewat ledger, disatukan agar tak ada pemanggil yang lupa salah satunya.
pub(crate) async fn catat_batch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &NewBatch,
) -> AppResult<Uuid> {
    // Batch lahir dengan sisa nol dan `stock::tambah` menaikkannya ke `quantity`, sehingga `remaining_qty` punya satu penulis dan ledger menyebut batch yang datang.
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

/// Teks opsional form: spasi tepi dibuang dan yang kosong dianggap tak diisi, agar tak tersimpan sebagai string kosong yang tampak "ada".
pub(crate) fn bersihkan(nilai: Option<String>) -> Option<String> {
    nilai
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
