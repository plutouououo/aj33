//! Endpoint produk, varian, batch, kategori, dan riwayat ledger stok.

use super::repo::{
    self, BatchPatch, Category, NewBatch, Product, ProductBatch, ProductFilter, ProductPatch,
    Scope, StockAdjustment, Ubah,
};
use super::service;
use super::sku;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, patch};
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
            get(get_product)
                .patch(update_product)
                .delete(delete_product),
        )
        .route(
            "/products/{id}/batches",
            get(list_batches).post(create_batch),
        )
        .route(
            "/products/{id}/batches/{batch_id}",
            patch(update_batch).delete(delete_batch),
        )
        .route(
            "/products/{id}/stock-adjustments",
            get(list_stock_adjustments).post(adjust_stock),
        )
        .route("/batches", get(list_batches_tersedia))
        .route("/categories", get(list_categories).post(create_category))
        .route("/sku-codes", get(list_sku_codes).post(create_sku_code))
        .route("/sku-codes/{id}", delete(delete_sku_code))
}

// --- Daftar produk ---

#[derive(Debug, Deserialize)]
struct ListQuery {
    search: Option<String>,
    category_id: Option<Uuid>,
    /// Varian dari satu induk saja.
    parent_id: Option<Uuid>,
    /// `induk` = hanya produk induk, `terjual` = hanya yang bisa dijual, kosong = semua baris.
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

/// Batas atas agar satu request tak menarik seluruh tabel dan menghabiskan memori.
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
        search: service::bersihkan(q.search),
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

/// Produk beserta kebutuhan halaman detailnya dikirim sekali karena ketiganya selalu dibaca bersama.
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
    let product = service::ambil_produk(&state, id).await?;
    let variants = repo::list_variants(&state.pool, id).await?;
    let batches = repo::list_batches(&state.pool, id).await?;

    Ok(Json(ProductDetail {
        product,
        variants,
        batches,
    }))
}

// --- Membuat produk ---

#[derive(Debug, Deserialize)]
struct ProductCreateRequest {
    /// Nama identifikasi internal.
    name: String,
    /// Judul yang dipakai di marketplace. Boleh kosong.
    seo_name: Option<String>,
    /// Bahan SKU; SKU tak diterima dari client dan selalu dirakit di sini agar bentuknya seragam.
    brand_name: Option<String>,
    product_type: Option<String>,
    variant_grade: Option<String>,
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
    /// Harga beli per batch, bukan harga modal default produk.
    purchase_price: Option<Decimal>,
    /// Stok awal. Selalu masuk sebagai batch, jadi asalnya tercatat.
    #[serde(default)]
    stock_qty: i32,
    batch_number: Option<String>,
    expiry_date: Option<NaiveDate>,
    low_stock_threshold: Option<i32>,
    image_url: Option<String>,
    /// Rak batch pertama (milik batch, bukan produk, lihat migrasi 0016); ikut terbuang bersama batch bila stok awal nol.
    storage_location: Option<String>,
}

async fn create_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ProductCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Product>)> {
    user.require(&[Role::Owner])?;

    let id = service::create_product(
        &state,
        user.id,
        service::CreateProductInput {
            name: body.name,
            seo_name: body.seo_name,
            brand_name: body.brand_name,
            product_type: body.product_type,
            variant_grade: body.variant_grade,
            variant_size: body.variant_size,
            parent_id: body.parent_id,
            category_id: body.category_id,
            price: body.price,
            price_shopee: body.price_shopee,
            price_tiktok: body.price_tiktok,
            cost_price: None,
            // Endpoint manual selalu menerbitkan produk langsung; toggle terbit hanya ada di jalur impor massal.
            is_active: true,
            low_stock_threshold: body.low_stock_threshold,
            image_url: body.image_url,
            stock_qty: body.stock_qty,
            purchase_price: body.purchase_price,
            batch_number: body.batch_number,
            expiry_date: body.expiry_date,
            storage_location: body.storage_location,
        },
    )
    .await?;

    let product = service::ambil_produk(&state, id).await?;
    Ok((axum::http::StatusCode::CREATED, Json(product)))
}

// --- Menyunting dan menghapus produk ---

/// `Option<Option<T>>`: tak disebut = biarkan, `null` = kosongkan (lihat `repo::Ubah`).
#[derive(Debug, Deserialize)]
struct ProductUpdateRequest {
    name: Option<String>,
    /// Koreksi SKU hanya untuk membetulkan salah ketik selama produk belum bergerak (SKU dirakit otomatis dan dibeku); tak disebut = biarkan.
    sku: Option<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    seo_name: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    brand_name: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    product_type: Ubah<String>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    variant_grade: Ubah<String>,
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
    is_active: Option<bool>,
}

async fn update_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductUpdateRequest>,
) -> AppResult<Json<Product>> {
    user.require(&[Role::Owner])?;

    // Hanya dibutuhkan `koreksi_sku` di bawah (SKU dan status sekarang); validasi harga/kategori tanggung jawab `service::update_product`.
    let sekarang = service::ambil_produk(&state, id).await?;

    let name = body
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let brand_name = ubah_teks(body.brand_name);
    let product_type = ubah_teks(body.product_type);
    let variant_grade = ubah_teks(body.variant_grade);
    let variant_size = ubah_teks(body.variant_size);

    // SKU tak dirakit ulang meski atribut berubah, karena memutus label cetak, pemetaan listing marketplace, dan hafalan pegawai; yang tersisa koreksi eksplisit dengan syarat di bawah.
    let sku = match body.sku {
        None => None,
        Some(diminta) => Some(Some(koreksi_sku(&state, &sekarang, &diminta).await?)),
    };

    let patch = ProductPatch {
        name,
        seo_name: ubah_teks(body.seo_name),
        sku,
        brand_name,
        product_type,
        variant_grade,
        variant_size,
        category_id: body.category_id,
        price: body.price,
        price_shopee: body.price_shopee,
        price_tiktok: body.price_tiktok,
        cost_price: body.cost_price,
        low_stock_threshold: body.low_stock_threshold,
        image_url: ubah_teks(body.image_url),
        is_active: body.is_active,
    };

    if !service::update_product(&state, id, patch).await? {
        return Err(AppError::not_found("Produk tidak ditemukan."));
    }

    Ok(Json(service::ambil_produk(&state, id).await?))
}

/// Menghapus produk sungguhan selama tak tersangkut varian, tiket, pesanan marketplace, listing, atau daftar belanja; riwayat kasir tetap utuh lewat snapshot nama dan harga di item transaksi.
async fn delete_product(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    let produk = service::ambil_produk(&state, id).await?;

    if let Some(penahan) = repo::penahan_hapus(&state.pool, id).await? {
        return Err(AppError::conflict(format!(
            "\"{}\" tidak bisa dihapus karena {penahan}. Nonaktifkan saja.",
            produk.name
        )));
    }

    if !repo::delete_product(&state.pool, id).await? {
        return Err(AppError::not_found("Produk tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Batch barang masuk ---

/// Batch bersisa seluruh produk jual, terbuka untuk semua peran login karena kasir memilih batch saat checkout dan isinya tak memuat harga pokok.
async fn list_batches_tersedia(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<ProductBatch>>> {
    Ok(Json(repo::list_batches_tersedia(&state.pool).await?))
}

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
    purchase_price: Option<Decimal>,
    quantity: i32,
    /// Boleh kosong untuk barang yang memang tidak punya kedaluwarsa.
    expiry_date: Option<NaiveDate>,
    /// Rak tempat kiriman ini ditaruh. Boleh kosong.
    storage_location: Option<String>,
}

/// Mencatat barang masuk; stok bertambah lewat ledger di transaksi yang sama sehingga jumlah batch dan stok tak pernah berselisih.
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

    let produk = service::ambil_produk(&state, id).await?;
    if produk.variant_count > 0 {
        return Err(AppError::bad_request(
            "Produk ini punya varian. Catat batch pada variannya, bukan di induk.",
        ));
    }

    let batch_number = service::bersihkan(body.batch_number);
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
    let batch_id = service::catat_batch(
        &mut tx,
        &NewBatch {
            product_id: id,
            batch_number,
            purchase_price: body.purchase_price,
            quantity: body.quantity,
            expiry_date: body.expiry_date,
            storage_location: service::bersihkan(body.storage_location),
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

#[derive(Debug, Deserialize)]
struct BatchPatchRequest {
    /// Ketiganya `Ubah`: owner harus bisa membatalkan angka/tanggal salah ketik, dan "kosong" bermakna (harga beli belum diketahui, tanpa kedaluwarsa, rak belum ditentukan).
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    purchase_price: Ubah<Decimal>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    expiry_date: Ubah<NaiveDate>,
    #[serde(default, deserialize_with = "repo::ubah_terkirim")]
    storage_location: Ubah<String>,
}

/// Mengoreksi catatan batch (harga beli, kedaluwarsa, rak) tanpa membongkar stok; jumlah tak bisa diubah karena melewati ledger, batch salah jumlah dibatalkan lalu dicatat ulang.
async fn update_batch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, batch_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<BatchPatchRequest>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    if body
        .purchase_price
        .flatten()
        .is_some_and(|h| h.is_sign_negative())
    {
        return Err(AppError::bad_request("Harga beli tidak boleh negatif."));
    }

    let patch = BatchPatch {
        purchase_price: body.purchase_price,
        expiry_date: body.expiry_date,
        storage_location: ubah_teks(body.storage_location),
    };

    if !repo::update_batch(&state.pool, id, batch_id, &patch).await? {
        return Err(AppError::not_found("Batch tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Membatalkan batch: baris dihapus dan stoknya ditarik lewat ledger; ditolak bila stok tak cukup karena sebagian sudah terjual.
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

    // Ditarik dari batch itu sendiri sebelum baris dihapus (`kurangi` perlu sisanya), jadi pembatalan yang sebagian terjual ditolak dengan menyebut batchnya.
    stock::kurangi(
        &mut tx,
        &[StockLine {
            product_id: id,
            qty: batch.quantity,
            batch_id: Some(batch.id),
        }],
        StockReason::Restock,
        id,
        Some(user.id),
    )
    .await?;

    repo::delete_batch(&mut tx, batch.id).await?;

    tx.commit().await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// --- Ledger stok ---

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

/// Koreksi stok manual owner (selisih opname, barang rusak); barang masuk lewat batch agar kedaluwarsanya tercatat.
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

    // Tanpa `batch_id`: koreksi turun memakai FEFO, koreksi naik dibuatkan batch tanpa asal oleh `stock::tambah`, karena opname tak tahu kiriman mana yang selisih.
    let line = StockLine {
        product_id: id,
        qty: body.change_qty.abs(),
        batch_id: None,
    };

    let mut tx = state.pool.begin().await?;

    // `reference_id` menunjuk produk itu sendiri (koreksi manual tak berasal dari transaksi/order) agar baris ledger selalu punya rujukan.
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

    Ok(Json(service::ambil_produk(&state, id).await?))
}

// --- Kategori ---

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

// Perkakas bersama (`ambil_produk`, `periksa_harga`, `rakit_sku`, `catat_batch`, `bersihkan`) ada di `service.rs` agar commit impor memakai aturan yang sama dengan handler.

/// SKU hasil koreksi manual selama produk masih boleh dikoreksi (`repo::penahan_ubah_sku`); SKU kosong dikecualikan agar produk lama bisa diberi SKU.
async fn koreksi_sku(state: &AppState, sekarang: &Product, diminta: &str) -> AppResult<String> {
    if sekarang.sku.is_some() {
        if let Some(penahan) = repo::penahan_ubah_sku(&state.pool, sekarang.id).await? {
            return Err(AppError::conflict(format!(
                "SKU tidak bisa diubah karena produk {penahan}."
            )));
        }
    }

    // Dinormalkan dulu agar koreksi tak melahirkan penyimpangan bentuk; `normalkan` juga menegakkan panjang 6-12 dan karakter yang boleh, sama dengan hasil rakitan.
    let kode = sku::normalkan(diminta).map_err(|err| AppError::bad_request(err.to_string()))?;

    repo::sku_harus_bebas(&state.pool, &kode, Some(sekarang.id)).await?;
    Ok(kode)
}

// --- Kamus kode SKU ---

/// Kamus dibaca siapa pun yang login (halaman produk memakainya menjelaskan SKU ditolak), hanya Owner yang mengubah.
async fn list_sku_codes(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<repo::SkuCode>>> {
    Ok(Json(repo::list_sku_codes(&state.pool).await?))
}

#[derive(Debug, Deserialize)]
struct SkuCodeRequest {
    /// `jenis`, `grade`, `merek`, atau `ukuran`.
    kind: String,
    /// Nilai atribut apa adanya, mis. "SP 08".
    source: String,
    code: String,
}

async fn create_sku_code(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<SkuCodeRequest>,
) -> AppResult<(axum::http::StatusCode, Json<repo::SkuCode>)> {
    user.require(&[Role::Owner])?;

    let kind = sku::Bagian::parse(body.kind.trim()).ok_or_else(|| {
        AppError::bad_request("Bagian harus salah satu dari: jenis, grade, merek, ukuran.")
    })?;

    let source = body.source.trim();
    if source.is_empty() {
        return Err(AppError::bad_request("Nilai atributnya wajib diisi."));
    }
    // Nilai tanpa satu pun huruf/angka tak punya kunci pencarian sehingga tak akan pernah ditemukan saat merakit.
    if sku::kunci(source).is_empty() {
        return Err(AppError::bad_request(
            "Nilai atribut harus berisi huruf atau angka.",
        ));
    }

    // Hanya huruf besar-kecil yang dinormalkan, bukan tanda baca: "s08" jelas maksudnya, tetapi "S-08" akan melahirkan SKU berbagian lebih dari tiga dan membuangnya diam-diam menyimpan kode yang berbeda.
    let code = body.code.trim().to_uppercase();
    sku::periksa_kode_kamus(&code).map_err(|err| AppError::bad_request(err.to_string()))?;

    let entri = repo::insert_sku_code(&state.pool, kind, source, &code, user.id).await?;
    Ok((axum::http::StatusCode::CREATED, Json(entri)))
}

/// Menghapus entri kamus tak mengubah SKU yang sudah dirakit (beku sejak dibuat), hanya produk yang dibuat sesudahnya.
async fn delete_sku_code(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    if !repo::delete_sku_code(&state.pool, id).await? {
        return Err(AppError::not_found("Entri kamus tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `bersihkan` untuk kolom yang boleh dikosongkan: "" dari form dibaca sebagai permintaan mengosongkan.
fn ubah_teks(nilai: Ubah<String>) -> Ubah<String> {
    nilai.map(service::bersihkan)
}
