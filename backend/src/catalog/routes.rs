//! Endpoint produk, kategori, dan riwayat ledger stok.

use super::repo::{
    self, Category, NewProduct, Product, ProductFilter, ProductPatch, StockAdjustment,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/products", get(list_products).post(create_product))
        .route("/products/{id}", get(get_product).patch(update_product))
        .route(
            "/products/{id}/stock-adjustments",
            get(list_stock_adjustments).post(adjust_stock),
        )
        .route("/categories", get(list_categories).post(create_category))
}

// ---------------------------------------------------------------------
// Produk
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListQuery {
    search: Option<String>,
    category_id: Option<Uuid>,
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

    let search = q
        .search
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let filter = ProductFilter {
        search,
        category_id: q.category_id,
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

async fn get_product(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Product>> {
    let product = repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

    Ok(Json(product))
}

#[derive(Debug, Deserialize)]
struct ProductCreateRequest {
    name: String,
    sku: Option<String>,
    category_id: Option<Uuid>,
    /// Harga dasar, yang dipakai kasir di toko.
    price: Decimal,
    /// Harga di marketplace. Kosong berarti belum diatur, bukan gratis.
    price_shopee: Option<Decimal>,
    /// Mencakup Tokopedia -- satu kanal dengan TikTok Shop.
    price_tiktok: Option<Decimal>,
    cost_price: Option<Decimal>,
    #[serde(default)]
    stock_qty: i32,
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
    if body.price.is_sign_negative() {
        return Err(AppError::bad_request("Harga tidak boleh negatif."));
    }
    // Diperiksa di sini supaya pesannya menyebut marketplace mana yang
    // salah. CHECK constraint di database tetap ada sebagai jaring terakhir,
    // tapi galatnya tidak bisa menyebutkan itu.
    if body.price_shopee.is_some_and(|h| h.is_sign_negative()) {
        return Err(AppError::bad_request("Harga Shopee tidak boleh negatif."));
    }
    if body.price_tiktok.is_some_and(|h| h.is_sign_negative()) {
        return Err(AppError::bad_request(
            "Harga Tokopedia/TikTok Shop tidak boleh negatif.",
        ));
    }
    if body.stock_qty < 0 {
        return Err(AppError::bad_request("Stok awal tidak boleh negatif."));
    }

    let sku = body
        .sku
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let Some(sku) = &sku {
        if repo::sku_dipakai(&state.pool, sku, None).await? {
            return Err(AppError::bad_request(format!(
                "SKU \"{sku}\" sudah dipakai."
            )));
        }
    }

    if let Some(category_id) = body.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    let id = repo::insert_product(
        &state.pool,
        &NewProduct {
            name: name.to_string(),
            sku,
            category_id: body.category_id,
            price: body.price,
            price_shopee: body.price_shopee,
            price_tiktok: body.price_tiktok,
            cost_price: body.cost_price,
            stock_qty: body.stock_qty,
            low_stock_threshold: body.low_stock_threshold.unwrap_or(5),
            image_url: body.image_url,
            unit: body.unit,
            storage_location: bersihkan(body.storage_location),
            created_by: user.id,
        },
    )
    .await?;

    let product = repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

    Ok((axum::http::StatusCode::CREATED, Json(product)))
}

#[derive(Debug, Deserialize)]
struct ProductUpdateRequest {
    name: Option<String>,
    sku: Option<String>,
    category_id: Option<Uuid>,
    price: Option<Decimal>,
    price_shopee: Option<Decimal>,
    price_tiktok: Option<Decimal>,
    cost_price: Option<Decimal>,
    low_stock_threshold: Option<i32>,
    image_url: Option<String>,
    unit: Option<String>,
    storage_location: Option<String>,
    is_active: Option<bool>,
}

async fn update_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductUpdateRequest>,
) -> AppResult<Json<Product>> {
    user.require(&[Role::Owner])?;

    if let Some(sku) = body.sku.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if repo::sku_dipakai(&state.pool, sku, Some(id)).await? {
            return Err(AppError::bad_request(format!(
                "SKU \"{sku}\" sudah dipakai."
            )));
        }
    }

    if let Some(category_id) = body.category_id {
        if !repo::category_ada(&state.pool, category_id).await? {
            return Err(AppError::bad_request("Kategori tidak ditemukan."));
        }
    }

    let patch = ProductPatch {
        name: body
            .name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        sku: body.sku,
        category_id: body.category_id,
        price: body.price,
        price_shopee: body.price_shopee,
        price_tiktok: body.price_tiktok,
        cost_price: body.cost_price,
        low_stock_threshold: body.low_stock_threshold,
        image_url: body.image_url,
        unit: body.unit,
        storage_location: bersihkan(body.storage_location),
        is_active: body.is_active,
    };

    if !repo::update_product(&state.pool, id, &patch).await? {
        return Err(AppError::not_found("Produk tidak ditemukan."));
    }

    let product = repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

    Ok(Json(product))
}

/// Teks opsional dari form: spasi di tepi dibuang, dan yang tersisa kosong
/// diperlakukan sebagai tidak diisi. Tanpa ini, field yang dikosongkan
/// pengguna tersimpan sebagai string kosong dan tampil sebagai lokasi yang
/// "ada" tapi tidak menunjukkan apa pun.
fn bersihkan(nilai: Option<String>) -> Option<String> {
    nilai
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

/// Koreksi stok manual oleh Owner. Tetap lewat `stock.rs` supaya ikut
/// tercatat di ledger seperti perubahan stok lainnya.
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

    let product = repo::find_product(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

    Ok(Json(product))
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
