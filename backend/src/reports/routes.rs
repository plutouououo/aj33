//! Endpoint dasbor dan laporan khusus owner (omzet, laba, harga pokok adalah isi buku toko), dibatasi di sini bukan hanya di menu frontend.

use super::repo::{
    self, Counts, ExpenseSlice, LowStockProduct, MonthlyPoint, SaleRow, SalesFilter, SalesSummary,
    StockValue, TopProduct, TransactionDetail,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/reports/sales", get(sales_report))
        .route("/reports/sales/{id}", get(sales_transaction_detail))
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

/// Menerjemahkan periode ke satuan `date_trunc` berupa `&'static str`, sehingga teks pengguna tak pernah sampai ke query dan nilai tak dikenal ditolak.
fn satuan_periode(period: Option<&str>) -> AppResult<Option<&'static str>> {
    match period.unwrap_or("month") {
        "today" => Ok(Some("day")),
        "week" => Ok(Some("week")),
        "month" => Ok(Some("month")),
        "year" => Ok(Some("year")),
        "all" => Ok(None),
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

impl ReportQuery {
    /// Mengembalikan saringan utama, saringan Produk Terlaris (periode boleh beda, metode bayar dan jenis transaksi ikut saringan utama), dan jendela halaman kedua daftar.
    fn menjadi_filter(self) -> AppResult<(SalesFilter, SalesFilter, Halaman)> {
        let period = satuan_periode(self.period.as_deref())?;
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

        let filter = SalesFilter {
            period,
            payment_method: payment_method.clone(),
            transaction_type: transaction_type.clone(),
        };

        let top_period = match self.top_period.as_deref() {
            Some(p) => satuan_periode(Some(p))?,
            None => period,
        };
        let top_filter = SalesFilter {
            period: top_period,
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
        repo::sales_summary_previous(&state.pool, &SalesFilter::periode(Some("day")))
            .await?
            .map(|s| s.revenue)
            .unwrap_or_default();
    let last_month_revenue =
        repo::sales_summary_previous(&state.pool, &SalesFilter::periode(Some("month")))
            .await?
            .map(|s| s.revenue)
            .unwrap_or_default();

    // Daftar penjualan terakhir tanpa batas periode karena dasbor kosong sepanjang pagi tak memberi tahu apa pun.
    let recent_sales =
        repo::recent_sales(&state.pool, &SalesFilter::periode(None), DASBOR_DAFTAR, 0).await?;
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
    trend: Vec<MonthlyPoint>,
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
        trend: repo::monthly_trend(&state.pool, &filter).await?,
        expense_breakdown: repo::expense_breakdown(&state.pool, &filter).await?,
        top_products: repo::top_products(
            &state.pool,
            &top_filter,
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
