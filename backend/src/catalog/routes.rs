//! Endpoint produk, varian, batch, kategori, dan riwayat ledger stok.

use super::repo::{
    self, Category, NewBatch, NewProduct, Product, ProductBatch, ProductFilter, ProductPatch,
    Scope, StockAdjustment, Ubah,
};
use super::sku;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/products", get(list_products).post(create_product))
        .route(
            "/products/{id}",
            get(get_product).patch(update_product).delete(delete_product),
        )
        .route(
            "/products/{id}/batches",
            get(list_batches).post(create_batch),
        )
        .route("/products/{id}/batches/{batch_id}", delete(delete_batch))
        .route(
            "/products/{id}/stock-adjustments",
            get(list_stock_adjustments).post(adjust_stock),
        )
        .route("/categories", get(list_categories).post(create_category))
}

// ---------------------------------------------------------------------
// Daftar produk
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListQuery {
    search: Option<String>,
    category_id: Option<Uuid>,
    /// Varian dari satu induk saja.
    parent_id: Option<Uuid>,
    /// `induk` = hanya produk induk, `terjual` = hanya yang benar-benar bisa
    /// dijual. Kosong berarti semua baris.
    scope: Option<String>,
    #[serde(default)]
    include_inactive: bool,
    page: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct PaginatedProducts {
    data: Vec<Product>,
    page: i64,
    limit: i64,
    total: i64,
}

/// Batas atas supaya satu request tidak bisa menarik seluruh tabel dan
/// menghabiskan memori server.
const LIMIT_MAKS: i64 = 200;

async fn list_products(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<PaginatedProducts>> {
    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(50).clamp(1, LIMIT_MAKS);

    let scope = match q.scope.as_deref() {
        None | Some("") | Some("semua") => Scope::Semua,
        Some("induk") => Scope::Induk,
        Some("terjual") => Scope::Terjual,
        Some(lain) => {
            return Err(AppError::bad_request(format!(
                "Nilai scope \"{lain}\" tidak dikenal."
            )))
        }
    };

    let filter = ProductFilter {
        search: bersihkan(q.search),
        category_id: q.category_id,
        parent_id: q.parent_id,
        scope,
        only_active: !q.include_inactive,
        limit,
        offset: (page - 1) * limit,
    };

    let (data, total) = repo::list_products(&state.pool, &filter).await?;

    Ok(Json(PaginatedProducts {
        data,
        page,
        limit,
        total,
    }))
}

/// Produk beserta segala yang dibutuhkan halaman detailnya. Dikirim sekali
/// jalan karena ketiganya selalu dibaca bersama -- memecahnya jadi tiga
/// endpoint hanya menambah perjalanan bolak-balik tanpa menambah kegunaan.
#[derive(Debug, Serialize)]
struct ProductDetail {
    #[serde(flatten)]
    product: Product,
    variants: Vec<Product>,
    batches: Vec<ProductBatch>,
}

async fn get_product(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ProductDetail>> {
    let product = ambil_produk(&state, id).await?;
    let variants = repo::list_variants(&state.pool, id).await?;
    let batches = repo::list_batches(&state.pool, id).await?;

    Ok(Json(ProductDetail {
        product,
        variants,
        batches,
    }))
}

// ---------------------------------------------------------------------
// Membuat produk
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ProductCreateRequest {
    /// Nama identifikasi internal.
    name: String,
    /// Judul yang dipakai di marketplace. Boleh kosong.
    seo_name: Option<String>,
    /// Bahan SKU. SKU sendiri tidak diterima dari client -- selalu dirakit
    /// di sini, supaya bentuknya sama untuk semua produk.
    brand_name: Option<String>,
    product_type: Option<String>,
    variant_color: Option<String>,
    variant_size: Option<String>,
    /// Terisi berarti produk ini varian dari produk lain.
    parent_id: Option<Uuid>,
    category_id: Option<Uuid>,
    /// Harga dasar, yang dipakai kasir di toko.
    price: Decimal,
    /// Harga di marketplace. Kosong berarti belum diatur, bukan gratis.
    price_shopee: Option<Decimal>,
    /// Mencakup Tokopedia -- satu kanal dengan TikTok Shop.
    price_tiktok: Option<Decimal>,
    cost_price: Option<Decimal>,
    /// Stok awal. Selalu masuk sebagai batch, jadi asalnya tercatat.
    #[serde(default)]
    stock_qty: i32,
    batch_number: Option<String>,
    expiry_date: Option<NaiveDate>,
    low_stock_threshold: Option<i32>,
    image_url: Option<String>,
    unit: Option<String>,
    /// Label rak internal yang dibaca pengepak.
    storage_location: Option<String>,
}

async fn create_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ProductCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Product>)> {
    user.require(&[Role::Owner])?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("Nama produk wajib diisi."));
    }
    periksa_harga(body.price, body.price_shopee, body.price_tiktok)?;
    if body.stock_qty < 0 {
        return Err(AppError::bad_request("Stok awal tidak boleh negatif."));
    }

    // Varian menempel pada induk yang harus benar-benar ada, dan induk itu
    // tidak boleh varian: katalog bertingkat-tingkat tidak punya wujud yang
    // masuk akal di layar kasir maupun di marketplace.
    if let Some(parent_id) = body.parent_id {
        let induk = ambil_produk(&state, parent_id).await?;
        if induk.parent_id.is_some() {
            return Err(AppError::bad_request(
                "Varian tidak bisa punya varian lagi.",
            ));
        }
    }

    if let Some(category_id) = body.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    let brand_name = bersihkan(body.brand_name);
    let product_type = bersihkan(body.product_type);
    let variant_color = bersihkan(body.variant_color);
    let variant_size = bersihkan(body.variant_size);

    let sku = rakit_sku(
        &state,
        &brand_name,
        &product_type,
        &variant_color,
        &variant_size,
        name,
        None,
    )
    .await?;

    let batch_number = bersihkan(body.batch_number);

    // Produk, batch pertamanya, dan baris ledger stok ditulis dalam satu
    // transaksi: produk yang tersimpan tanpa stok awalnya akan tampil habis
    // padahal barangnya ada di rak.
    let mut tx = state.pool.begin().await?;

    let id = repo::insert_product(
        &mut tx,
        &NewProduct {
            name: name.to_string(),
            seo_name: bersihkan(body.seo_name),
            sku: Some(sku),
            brand_name,
            product_type,
            variant_color,
            variant_size,
            parent_id: body.parent_id,
            category_id: body.category_id,
            price: body.price,
            price_shopee: body.price_shopee,
            price_tiktok: body.price_tiktok,
            cost_price: body.cost_price,
            low_stock_threshold: body.low_stock_threshold.unwrap_or(5),
            image_url: bersihkan(body.image_url),
            unit: bersihkan(body.unit),
            storage_location: bersihkan(body.storage_location),
            created_by: user.id,
        },
    )
    .await?;

    if body.stock_qty > 0 {
        catat_batch(
            &mut tx,
            &NewBatch {
                product_id: id,
                batch_number,
                quantity: body.stock_qty,
                expiry_date: body.expiry_date,
                created_by: user.id,
            },
        )
        .await?;
    }

    tx.commit().await?;

    let product = ambil_produk(&state, id).await?;
    Ok((axum::http::StatusCode::CREATED, Json(product)))
}

// ---------------------------------------------------------------------
// Menyunting dan menghapus produk
// ---------------------------------------------------------------------

/// Field bertipe `Option<Option<T>>`: tidak disebut berarti "biarkan", `null`
/// berarti "kosongkan". Lihat `repo::Ubah`.
#[derive(Debug, Deserialize)]
struct ProductUpdateRequest {
    name: Option<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    seo_name: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    brand_name: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    product_type: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    variant_color: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    variant_size: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    category_id: Ubah<Uuid>,
    price: Option<Decimal>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    price_shopee: Ubah<Decimal>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    price_tiktok: Ubah<Decimal>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    cost_price: Ubah<Decimal>,
    low_stock_threshold: Option<i32>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    image_url: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    unit: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    storage_location: Ubah<String>,
    is_active: Option<bool>,
}

async fn update_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductUpdateRequest>,
) -> AppResult<Json<Product>> {
    user.require(&[Role::Owner])?;

    let sekarang = ambil_produk(&state, id).await?;

    periksa_harga(
        body.price.unwrap_or(sekarang.price),
        body.price_shopee.flatten(),
        body.price_tiktok.flatten(),
    )?;

    if let Some(Some(category_id)) = body.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    let name = body
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let brand_name = ubah_teks(body.brand_name);
    let product_type = ubah_teks(body.product_type);
    let variant_color = ubah_teks(body.variant_color);
    let variant_size = ubah_teks(body.variant_size);

    // SKU selalu ikut atribut pembentuknya. Kalau tidak dirakit ulang di
    // sini, produk yang warnanya diperbaiki akan menyimpan SKU yang
    // menyebut warna lamanya -- dan SKU yang berbohong lebih buruk daripada
    // SKU yang tidak ada.
    let sku = if brand_name.is_some()
        || product_type.is_some()
        || variant_color.is_some()
        || variant_size.is_some()
        || name.is_some()
    {
        let terpakai = |ubah: &Ubah<String>, lama: &Option<String>| -> Option<String> {
            match ubah {
                None => lama.clone(),
                Some(isi) => isi.clone(),
            }
        };

        let sku = rakit_sku(
            &state,
            &terpakai(&brand_name, &sekarang.brand_name),
            &terpakai(&product_type, &sekarang.product_type),
            &terpakai(&variant_color, &sekarang.variant_color),
            &terpakai(&variant_size, &sekarang.variant_size),
            name.as_deref().unwrap_or(&sekarang.name),
            Some(id),
        )
        .await?;
        Some(Some(sku))
    } else {
        None
    };

    let patch = ProductPatch {
        name,
        seo_name: ubah_teks(body.seo_name),
        sku,
        brand_name,
        product_type,
        variant_color,
        variant_size,
        category_id: body.category_id,
        price: body.price,
        price_shopee: body.price_shopee,
        price_tiktok: body.price_tiktok,
        cost_price: body.cost_price,
        low_stock_threshold: body.low_stock_threshold,
        image_url: ubah_teks(body.image_url),
        unit: ubah_teks(body.unit),
        storage_location: ubah_teks(body.storage_location),
        is_active: body.is_active,
    };

    if !repo::update_product(&state.pool, id, &patch).await? {
        return Err(AppError::not_found("Produk tidak ditemukan."));
    }

    Ok(Json(ambil_produk(&state, id).await?))
}

/// Menghapus produk sungguhan, bukan menonaktifkannya. Hanya mungkin selama
/// produk itu belum tersangkut di mana-mana: begitu pernah terjual atau
/// dipetik pengepak, menghapusnya akan melubangi riwayat yang dipakai
/// menghitung omzet, jadi yang tersisa adalah menonaktifkan.
async fn delete_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    ambil_produk(&state, id).await?;

    if let Some(penahan) = repo::penahan_hapus(&state.pool, id).await? {
        return Err(AppError::conflict(format!(
            "Produk tidak bisa dihapus karena {penahan}. Nonaktifkan saja."
        )));
    }

    if !repo::delete_product(&state.pool, id).await? {
        return Err(AppError::not_found("Produk tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------
// Batch barang masuk
// ---------------------------------------------------------------------

async fn list_batches(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<ProductBatch>>> {
    Ok(Json(repo::list_batches(&state.pool, id).await?))
}

#[derive(Debug, Deserialize)]
struct BatchCreateRequest {
    batch_number: Option<String>,
    quantity: i32,
    /// Boleh kosong untuk barang yang memang tidak punya kedaluwarsa.
    expiry_date: Option<NaiveDate>,
}

/// Mencatat barang masuk. Stoknya bertambah lewat ledger di transaksi yang
/// sama, jadi jumlah batch dan stok berjalan tidak pernah bisa berselisih.
async fn create_batch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<BatchCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<ProductBatch>)> {
    user.require(&[Role::Owner])?;

    if body.quantity <= 0 {
        return Err(AppError::bad_request("Jumlah batch harus lebih dari nol."));
    }

    let produk = ambil_produk(&state, id).await?;
    if produk.variant_count > 0 {
        return Err(AppError::bad_request(
            "Produk ini punya varian. Catat batch pada variannya, bukan di induk.",
        ));
    }

    let batch_number = bersihkan(body.batch_number);
    if let Some(nomor) = &batch_number {
        if repo::list_batches(&state.pool, id)
            .await?
            .iter()
            .any(|b| b.batch_number.as_deref() == Some(nomor.as_str()))
        {
            return Err(AppError::conflict(format!(
                "Batch \"{nomor}\" sudah pernah dicatat untuk produk ini."
            )));
        }
    }

    let mut tx = state.pool.begin().await?;
    let batch_id = catat_batch(
        &mut tx,
        &NewBatch {
            product_id: id,
            batch_number,
            quantity: body.quantity,
            expiry_date: body.expiry_date,
            created_by: user.id,
        },
    )
    .await?;
    tx.commit().await?;

    let batch = repo::list_batches(&state.pool, id)
        .await?
        .into_iter()
        .find(|b| b.id == batch_id)
        .ok_or_else(|| AppError::not_found("Batch tidak ditemukan."))?;

    Ok((axum::http::StatusCode::CREATED, Json(batch)))
}

/// Membatalkan pencatatan batch: barisnya dihapus dan stok yang dulu
/// ditambahkannya ditarik kembali lewat ledger. Ditolak kalau stok yang ada
/// sudah tidak cukup -- artinya sebagian barang batch itu sudah terjual, dan
/// yang terjual tidak bisa dianggap tidak pernah masuk.
async fn delete_batch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, batch_id)): Path<(Uuid, Uuid)>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    let mut tx = state.pool.begin().await?;

    let batch = repo::find_batch(&mut tx, id, batch_id)
        .await?
        .ok_or_else(|| AppError::not_found("Batch tidak ditemukan."))?;

    repo::delete_batch(&mut tx, batch.id).await?;

    stock::kurangi(
        &mut tx,
        &[StockLine {
            product_id: id,
            qty: batch.quantity,
        }],
        StockReason::Restock,
        id,
        Some(user.id),
    )
    .await?;

    tx.commit().await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------
// Ledger stok
// ---------------------------------------------------------------------

async fn list_stock_adjustments(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<StockAdjustment>>> {
    user.require(&[Role::Owner])?;
    let rows = repo::list_stock_adjustments(&state.pool, id, 100).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct StockAdjustmentRequest {
    /// Positif menambah, negatif mengurangi.
    change_qty: i32,
}

/// Koreksi stok manual oleh Owner -- selisih hasil opname, barang rusak, dan
/// sejenisnya. Barang MASUK tidak lewat sini melainkan lewat batch, supaya
/// tanggal kedaluwarsanya ikut tercatat.
async fn adjust_stock(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StockAdjustmentRequest>,
) -> AppResult<Json<Product>> {
    user.require(&[Role::Owner])?;

    if body.change_qty == 0 {
        return Err(AppError::bad_request("Perubahan stok tidak boleh nol."));
    }

    let line = StockLine {
        product_id: id,
        qty: body.change_qty.abs(),
    };

    let mut tx = state.pool.begin().await?;

    // `reference_id` menunjuk produk itu sendiri: koreksi manual tidak
    // berasal dari transaksi atau order mana pun, tapi kolomnya tetap diisi
    // agar baris ledger selalu punya rujukan yang bisa ditelusuri.
    if body.change_qty > 0 {
        stock::tambah(
            &mut tx,
            &[line],
            StockReason::ManualAdjustment,
            id,
            Some(user.id),
        )
        .await?;
    } else {
        stock::kurangi(
            &mut tx,
            &[line],
            StockReason::ManualAdjustment,
            id,
            Some(user.id),
        )
        .await?;
    }

    tx.commit().await?;

    Ok(Json(ambil_produk(&state, id).await?))
}

// ---------------------------------------------------------------------
// Kategori
// ---------------------------------------------------------------------

async fn list_categories(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<Category>>> {
    let rows = repo::list_categories(&state.pool).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct CategoryCreateRequest {
    name: String,
}

async fn create_category(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CategoryCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Category>)> {
    user.require(&[Role::Owner])?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("Nama kategori wajib diisi."));
    }

    if repo::category_nama_dipakai(&state.pool, name).await? {
        return Err(AppError::bad_request(format!(
            "Kategori \"{name}\" sudah ada."
        )));
    }

    let category = repo::insert_category(&state.pool, name, user.id).await?;
    Ok((axum::http::StatusCode::CREATED, Json(category)))
}

// ---------------------------------------------------------------------
// Perkakas bersama
// ---------------------------------------------------------------------

async fn ambil_produk(state: &AppState, id: Uuid) -> AppResult<Product> {
    repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))
}

/// Harga diperiksa di sini supaya pesannya menyebut marketplace mana yang
/// salah. CHECK constraint di database tetap ada sebagai jaring terakhir,
/// tapi galatnya tidak bisa menyebutkan itu.
fn periksa_harga(
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

/// SKU siap pakai: dirakit dari atributnya, lalu dipastikan belum dipakai
/// produk lain. Produk yang belum punya satu pun atribut jatuh ke namanya --
/// tanpa itu produk lama yang disunting akan kehilangan SKU-nya.
async fn rakit_sku(
    state: &AppState,
    brand_name: &Option<String>,
    product_type: &Option<String>,
    variant_color: &Option<String>,
    variant_size: &Option<String>,
    name: &str,
    kecuali: Option<Uuid>,
) -> AppResult<String> {
    let basis = sku::rakit(&[
        brand_name.as_deref(),
        product_type.as_deref(),
        variant_color.as_deref(),
        variant_size.as_deref(),
    ])
    .or_else(|| sku::rakit(&[Some(name)]))
    .ok_or_else(|| AppError::bad_request("Nama produk wajib diisi."))?;

    repo::sku_unik(&state.pool, &basis, kecuali).await
}

/// Menulis satu batch sekaligus menambah stoknya lewat ledger. Keduanya
/// selalu terjadi bersama, jadi disatukan di sini supaya tidak ada pemanggil
/// yang lupa salah satunya.
async fn catat_batch(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    input: &NewBatch,
) -> AppResult<Uuid> {
    let id = repo::insert_batch(tx, input).await?;

    stock::tambah(
        tx,
        &[StockLine {
            product_id: input.product_id,
            qty: input.quantity,
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
fn bersihkan(nilai: Option<String>) -> Option<String> {
    nilai
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `bersihkan` untuk kolom yang boleh dikosongkan: "" dari form dibaca
/// sebagai permintaan mengosongkan, bukan sebagai nilai kosong yang
/// tersimpan apa adanya.
fn ubah_teks(nilai: Ubah<String>) -> Ubah<String> {
    nilai.map(bersihkan)
}
