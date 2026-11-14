//! Endpoint dasbor dan laporan khusus owner (omzet, laba, harga pokok adalah isi buku toko), dibatasi di sini bukan hanya di menu frontend.

use super::repo::{
    self, hari_ini, ChannelSummary, Counts, ExpenseSlice, LabaRugi, LowStockProduct, Periode,
    Rentang, SaleRow, SalesFilter, SalesSummary, StockValue, TopProduct, TransactionDetail, Tren,
    UangMasuk, UrutanProduk,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/reports/sales", get(sales_report))
        .route("/reports/sales/{id}", get(sales_transaction_detail))
        .route("/reports/profit-loss", get(profit_loss))
        .route("/reports/products", get(product_report))
        .route("/reports/cash-in", get(cash_in))
        .route("/reports/stock", get(stock_report))
        .route("/reports/voids", get(void_report))
        .route("/sheet/harian", get(sheet_harian))
}

/// Banyaknya baris pada daftar pendek di dasbor.
const DASBOR_DAFTAR: i64 = 5;
/// Produk terlaris per halaman bila tak diminta lain.
const TERLARIS_BAKU: i64 = 5;
const TERLARIS_MAKS: i64 = 100;
/// Batas atas daftar penjualan di laporan: cukup untuk ekspor satu periode tapi berbatas agar satu request tak menarik seluruh tabel.
const PENJUALAN_MAKS: i64 = 1000;
const PENJUALAN_BAKU: i64 = 15;

// --- Saringan ---

#[derive(Debug, Deserialize)]
struct ReportQuery {
    /// `today`, `week`, `month`, `year`, atau `all`; kosong = bulan ini karena riwayat seluruhnya jarang menjawab pertanyaan siapa pun.
    period: Option<String>,
    /// Awal dan akhir (termasuk) tanggal pilihan `YYYY-MM-DD` zona toko; bila salah satunya diisi, menggantikan `period`.
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    payment_method: Option<String>,
    #[serde(rename = "type")]
    transaction_type: Option<String>,
    /// Banyaknya baris penjualan yang dikembalikan; halaman memakai nilai baku, ekspor meminta lebih.
    sales_limit: Option<i64>,
    /// Banyaknya baris penjualan yang dilewati, untuk halaman berikutnya.
    sales_offset: Option<i64>,
    top_limit: Option<i64>,
    top_offset: Option<i64>,
    /// Periode Produk Terlaris terpisah dari `period`; kosong = ikut `period`.
    top_period: Option<String>,
}

/// Menerjemahkan nama periode ke `Rentang`; nilai tak dikenal ditolak agar salah ketik tak menjadi laporan kosong.
fn rentang_periode(period: Option<&str>) -> AppResult<Rentang> {
    match period.unwrap_or("month") {
        "today" => Ok(Rentang::Periode(Periode::Hari)),
        "week" => Ok(Rentang::Periode(Periode::Pekan)),
        "month" => Ok(Rentang::Periode(Periode::Bulan)),
        "year" => Ok(Rentang::Periode(Periode::Tahun)),
        "all" => Ok(Rentang::Semua),
        lain => Err(AppError::bad_request(format!(
            "Periode '{lain}' tidak dikenal."
        ))),
    }
}

/// Nilai kolom yang dikunci CHECK di DB, diperiksa di sini agar saringan salah ketik dijawab 400 berpesan jelas, bukan laporan kosong diam-diam.
fn pilihan(nilai: Option<String>, sah: &[&str], nama: &str) -> AppResult<Option<String>> {
    let Some(nilai) = nilai
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };

    if sah.contains(&nilai.as_str()) {
        Ok(Some(nilai))
    } else {
        Err(AppError::bad_request(format!(
            "{nama} '{nilai}' tidak dikenal."
        )))
    }
}

/// Rentang dari kueri `period`/`from`/`to` yang sama untuk semua laporan; tanggal yang terisi menang atas `period`.
pub fn rentang_kueri(
    period: Option<&str>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> AppResult<Rentang> {
    if from.is_none() && to.is_none() {
        return rentang_periode(period);
    }
    if let (Some(dari), Some(sampai)) = (from, to) {
        if dari > sampai {
            return Err(AppError::bad_request(
                "Tanggal awal tidak boleh setelah tanggal akhir.",
            ));
        }
    }
    Ok(Rentang::Tanggal {
        dari: from,
        sampai: to,
    })
}

impl ReportQuery {
    /// Mengembalikan saringan utama, saringan Produk Terlaris (periode boleh beda, metode bayar dan jenis transaksi ikut saringan utama), dan jendela halaman kedua daftar.
    fn menjadi_filter(self) -> AppResult<(SalesFilter, SalesFilter, Halaman)> {
        let rentang = rentang_kueri(self.period.as_deref(), self.from, self.to)?;
        let payment_method = pilihan(
            self.payment_method,
            &["cash", "transfer", "ewallet"],
            "Metode pembayaran",
        )?;
        let transaction_type = pilihan(
            self.transaction_type,
            &["walk_in", "pre_order"],
            "Jenis transaksi",
        )?;

        let top_rentang = match self.top_period.as_deref() {
            Some(p) => rentang_periode(Some(p))?,
            None => rentang.clone(),
        };
        let filter = SalesFilter {
            rentang,
            payment_method: payment_method.clone(),
            transaction_type: transaction_type.clone(),
        };
        let top_filter = SalesFilter {
            rentang: top_rentang,
            payment_method,
            transaction_type,
        };

        let halaman = Halaman {
            sales_limit: self
                .sales_limit
                .unwrap_or(PENJUALAN_BAKU)
                .clamp(1, PENJUALAN_MAKS),
            sales_offset: self.sales_offset.unwrap_or(0).max(0),
            top_limit: self
                .top_limit
                .unwrap_or(TERLARIS_BAKU)
                .clamp(1, TERLARIS_MAKS),
            top_offset: self.top_offset.unwrap_or(0).max(0),
        };

        Ok((filter, top_filter, halaman))
    }
}

struct Halaman {
    sales_limit: i64,
    sales_offset: i64,
    top_limit: i64,
    top_offset: i64,
}

// --- Dasbor ---

#[derive(Debug, Serialize)]
struct Dashboard {
    today_revenue: rust_decimal::Decimal,
    month_revenue: rust_decimal::Decimal,
    /// Omzet kemarin untuk pil di "Omzet Hari Ini", lewat `sales_summary_previous` yang sama dengan Laporan (jendela `day` digeser sehari).
    yesterday_revenue: rust_decimal::Decimal,
    /// Omzet bulan lalu, untuk pil naik/turun di kartu "Omzet Bulan Ini".
    last_month_revenue: rust_decimal::Decimal,
    today_transaction_count: i64,
    product_count: i64,
    customer_count: i64,
    low_stock_count: i64,
    /// Potret persediaan sekarang (nilai dan berat), bukan angka periode.
    stock_value: StockValue,
    recent_sales: Vec<SaleRow>,
    low_stock_products: Vec<LowStockProduct>,
}

async fn dashboard(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Dashboard>> {
    user.require(&[Role::Owner])?;

    let omzet = repo::today_and_month(&state.pool).await?;
    let Counts {
        product_count,
        customer_count,
        low_stock_count,
    } = repo::counts(&state.pool).await?;

    // Dasbor tanpa saringan metode bayar/jenis transaksi, jadi `SalesFilter::periode` pas, sama dengan `today_and_month` dengan jendela digeser satu hari/bulan.
    let yesterday_revenue =
        repo::sales_summary_previous(&state.pool, &SalesFilter::periode(Periode::Hari))
            .await?
            .map(|s| s.revenue)
            .unwrap_or_default();
    let last_month_revenue =
        repo::sales_summary_previous(&state.pool, &SalesFilter::periode(Periode::Bulan))
            .await?
            .map(|s| s.revenue)
            .unwrap_or_default();

    // Daftar penjualan terakhir tanpa batas periode karena dasbor kosong sepanjang pagi tak memberi tahu apa pun.
    let recent_sales =
        repo::recent_sales(&state.pool, &SalesFilter::default(), DASBOR_DAFTAR, 0).await?;
    let low_stock_products = repo::low_stock(&state.pool, DASBOR_DAFTAR).await?;

    Ok(Json(Dashboard {
        today_revenue: omzet.today_revenue,
        month_revenue: omzet.month_revenue,
        yesterday_revenue,
        last_month_revenue,
        today_transaction_count: omzet.today_transaction_count,
        product_count,
        customer_count,
        low_stock_count,
        stock_value: repo::stock_value(&state.pool).await?,
        recent_sales,
        low_stock_products,
    }))
}

// --- Laporan penjualan ---

#[derive(Debug, Serialize)]
struct SalesReport {
    summary: SalesSummary,
    /// Angka periode setara sebelumnya untuk kartu trend, `null` bila "Seluruh Waktu" (lihat `repo::sales_summary_previous`).
    previous_summary: Option<SalesSummary>,
    /// Mengikuti seluruh saringan; butirnya (jam, hari, bulan) dipilih dari panjang periode.
    trend: Tren,
    /// Omzet dan laba per kanal (toko, Shopee, Tokopedia) pada periode dan saringan yang sama dengan `summary`.
    channels: Vec<ChannelSummary>,
    expense_breakdown: Vec<ExpenseSlice>,
    top_products: Vec<TopProduct>,
    /// Seluruh produk terlaris pada periodenya (bukan hanya halaman ini), dasar jumlah halaman.
    top_products_total: i64,
    sales: Vec<SaleRow>,
    /// Jumlah baris di `sales` dibanding batas yang diminta, agar halaman bisa mengatakan daftar dipotong, bukan hanya segitu penjualannya.
    sales_limit: i64,
}

async fn sales_report(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ReportQuery>,
) -> AppResult<Json<SalesReport>> {
    user.require(&[Role::Owner])?;

    let (filter, top_filter, halaman) = q.menjadi_filter()?;

    Ok(Json(SalesReport {
        summary: repo::sales_summary(&state.pool, &filter).await?,
        previous_summary: repo::sales_summary_previous(&state.pool, &filter).await?,
        trend: repo::tren(&state.pool, &filter).await?,
        channels: repo::channel_summaries(&state.pool, &filter).await?,
        expense_breakdown: repo::expense_breakdown(&state.pool, &filter).await?,
        top_products: repo::top_products(
            &state.pool,
            &top_filter,
            UrutanProduk::Terjual,
            halaman.top_limit,
            halaman.top_offset,
        )
        .await?,
        top_products_total: repo::top_products_count(&state.pool, &top_filter).await?,
        sales: repo::recent_sales(
            &state.pool,
            &filter,
            halaman.sales_limit,
            halaman.sales_offset,
        )
        .await?,
        sales_limit: halaman.sales_limit,
    }))
}

// --- Laba rugi ---

/// Laba rugi milik seluruh usaha, jadi hanya periode yang bisa dipilih (beban tak mengenal metode bayar maupun jenis pembeli).
#[derive(Debug, Deserialize)]
struct PeriodeQuery {
    period: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
struct ProfitLossReport {
    current: LabaRugi,
    /// Rentang setara tepat sebelumnya, `null` bila tak punya pembanding (lihat `SalesFilter::sebelumnya`).
    previous: Option<LabaRugi>,
    /// Rentang pembanding agar halaman bisa menamainya tanpa meniru hitungan `sebelumnya`.
    previous_from: Option<NaiveDate>,
    previous_to: Option<NaiveDate>,
}

async fn profit_loss(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PeriodeQuery>,
) -> AppResult<Json<ProfitLossReport>> {
    user.require(&[Role::Owner])?;

    let filter = SalesFilter {
        rentang: rentang_kueri(q.period.as_deref(), q.from, q.to)?,
        ..SalesFilter::default()
    };
    let sebelumnya = filter.sebelumnya();
    let previous = match &sebelumnya {
        Some(f) => Some(repo::laba_rugi(&state.pool, f).await?),
        None => None,
    };
    let (previous_from, previous_to) = sebelumnya.map(|f| f.jendela()).unwrap_or_default();

    Ok(Json(ProfitLossReport {
        current: repo::laba_rugi(&state.pool, &filter).await?,
        previous,
        previous_from,
        previous_to,
    }))
}

// --- Laba per produk ---

#[derive(Debug, Deserialize)]
struct ProductQuery {
    period: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    /// `qty` (baku), `revenue`, `profit`, atau `margin`.
    sort: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ProductReport {
    data: Vec<TopProduct>,
    page: i64,
    limit: i64,
    total: i64,
}

async fn product_report(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ProductQuery>,
) -> AppResult<Json<ProductReport>> {
    user.require(&[Role::Owner])?;

    let filter = SalesFilter {
        rentang: rentang_kueri(q.period.as_deref(), q.from, q.to)?,
        ..SalesFilter::default()
    };
    let urutan = UrutanProduk::parse(q.sort.as_deref()).ok_or_else(|| {
        AppError::bad_request(format!(
            "Urutan '{}' tidak dikenal.",
            q.sort.as_deref().unwrap_or_default()
        ))
    })?;
    let limit = q.limit.unwrap_or(25).clamp(1, TERLARIS_MAKS);
    let page = q.page.unwrap_or(1).max(1);

    Ok(Json(ProductReport {
        data: repo::top_products(&state.pool, &filter, urutan, limit, (page - 1) * limit).await?,
        page,
        limit,
        total: repo::top_products_count(&state.pool, &filter).await?,
    }))
}

// --- Uang masuk ---

async fn cash_in(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PeriodeQuery>,
) -> AppResult<Json<UangMasuk>> {
    user.require(&[Role::Owner])?;

    let filter = SalesFilter {
        rentang: rentang_kueri(q.period.as_deref(), q.from, q.to)?,
        ..SalesFilter::default()
    };
    Ok(Json(repo::uang_masuk(&state.pool, &filter).await?))
}

// --- Stok ---

#[derive(Debug, Deserialize)]
struct StockQuery {
    period: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    purchase_limit: Option<i64>,
    purchase_offset: Option<i64>,
    reduction_limit: Option<i64>,
    reduction_offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct StockReport {
    /// Potret persediaan sekarang, bukan angka periode.
    stock_value: StockValue,
    #[serde(flatten)]
    laporan: repo::LaporanStok,
}

/// Batas atas satu halaman daftar di tab Stok dan Void.
const DAFTAR_MAKS: i64 = 100;
const DAFTAR_BAKU: i64 = 10;

fn halaman_daftar(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(DAFTAR_BAKU).clamp(1, DAFTAR_MAKS),
        offset.unwrap_or(0).max(0),
    )
}

async fn stock_report(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StockQuery>,
) -> AppResult<Json<StockReport>> {
    user.require(&[Role::Owner])?;

    let filter = SalesFilter {
        rentang: rentang_kueri(q.period.as_deref(), q.from, q.to)?,
        ..SalesFilter::default()
    };
    let (beli_limit, beli_offset) = halaman_daftar(q.purchase_limit, q.purchase_offset);
    let (kurang_limit, kurang_offset) = halaman_daftar(q.reduction_limit, q.reduction_offset);
    let halaman = repo::HalamanStok {
        beli_limit,
        beli_offset,
        kurang_limit,
        kurang_offset,
    };

    Ok(Json(StockReport {
        stock_value: repo::stock_value(&state.pool).await?,
        laporan: repo::laporan_stok(&state.pool, &filter, &halaman).await?,
    }))
}

// --- Void ---

#[derive(Debug, Deserialize)]
struct VoidQuery {
    period: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn void_report(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<VoidQuery>,
) -> AppResult<Json<repo::LaporanVoid>> {
    user.require(&[Role::Owner])?;

    let filter = SalesFilter {
        rentang: rentang_kueri(q.period.as_deref(), q.from, q.to)?,
        ..SalesFilter::default()
    };
    let (limit, offset) = halaman_daftar(q.limit, q.offset);
    Ok(Json(
        repo::laporan_void(&state.pool, &filter, limit, offset).await?,
    ))
}

// --- Detail transaksi ---

async fn sales_transaction_detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<TransactionDetail>> {
    user.require(&[Role::Owner])?;

    let detail = repo::transaction_detail(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))?;

    Ok(Json(detail))
}

// --- Laporan harian untuk Google Sheets ---

/// Pemegang `SHEET_TOKEN`; tipe tersendiri agar token itu hanya membuka satu endpoint baca, bukan API lain.
struct PembacaSheet;

impl FromRequestParts<AppState> for PembacaSheet {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let diharap = &state.config.sheet_token;
        if diharap.is_empty() {
            return Err(AppError::not_found("Laporan sheet belum diaktifkan."));
        }

        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .unwrap_or_default();

        // Dibandingkan lewat hash agar waktu pembandingan tak membocorkan awalan token yang benar.
        if Sha256::digest(token.as_bytes()) == Sha256::digest(diharap.as_bytes()) {
            Ok(Self)
        } else {
            Err(AppError::unauthorized("Token sheet tidak valid."))
        }
    }
}

#[derive(Debug, Deserialize)]
struct HarianQuery {
    /// `YYYY-MM-DD` waktu Jakarta; kosong berarti hari ini.
    tanggal: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize)]
struct LaporanHarian {
    tanggal: chrono::NaiveDate,
    transaksi: Vec<repo::BarisHarian>,
    pengeluaran: Vec<repo::PengeluaranHarian>,
}

async fn sheet_harian(
    _pembaca: PembacaSheet,
    State(state): State<AppState>,
    Query(query): Query<HarianQuery>,
) -> AppResult<Json<LaporanHarian>> {
    let tanggal = query.tanggal.unwrap_or_else(hari_ini);
    let transaksi = repo::laporan_harian(&state.pool, tanggal).await?;
    let pengeluaran = repo::pengeluaran_harian(&state.pool, tanggal).await?;
    Ok(Json(LaporanHarian {
        tanggal,
        transaksi,
        pengeluaran,
    }))
}
