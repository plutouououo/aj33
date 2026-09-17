//! Endpoint dasbor dan laporan penjualan.
//!
//! Keduanya owner saja. Angka omzet, laba, dan harga pokok adalah isi buku
//! toko; kasir dan pengepak tidak punya urusan dengannya, dan pembatasannya
//! ada di sini -- bukan hanya di menu frontend, yang bisa dilewati siapa pun
//! yang mengetikkan alamatnya sendiri.

use super::repo::{
    self, Counts, ExpenseSlice, LowStockProduct, MonthlyPoint, SaleRow, SalesFilter, SalesSummary,
    TopProduct,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard))
        .route("/reports/sales", get(sales_report))
}

/// Banyaknya baris pada daftar pendek di dasbor.
const DASBOR_DAFTAR: i64 = 5;
/// Produk terlaris yang ditampilkan laporan.
const TERLARIS: i64 = 10;
/// Batas atas daftar penjualan pada laporan. Cukup besar untuk ekspor satu
/// periode, dan tetap berbatas supaya satu request tidak bisa menarik
/// seluruh tabel.
const PENJUALAN_MAKS: i64 = 1000;
const PENJUALAN_BAKU: i64 = 15;

// ---------------------------------------------------------------------
// Saringan
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ReportQuery {
    /// `today`, `week`, `month`, `year`, atau `all`. Kosong berarti bulan
    /// ini -- laporan yang terbuka dengan seluruh riwayat sekaligus jarang
    /// menjawab pertanyaan siapa pun.
    period: Option<String>,
    payment_method: Option<String>,
    #[serde(rename = "type")]
    transaction_type: Option<String>,
    /// Banyaknya baris penjualan yang ikut dikembalikan. Halaman memakai
    /// nilai baku; ekspor meminta lebih banyak.
    sales_limit: Option<i64>,
}

/// Menerjemahkan periode menjadi satuan `date_trunc` milik Postgres.
///
/// Hasilnya `&'static str`, jadi tidak ada teks dari pengguna yang pernah
/// sampai ke query -- nilai yang tidak dikenal ditolak di sini.
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

/// Nilai kolom yang dikunci CHECK constraint di database. Diperiksa di sini
/// supaya saringan yang salah ketik dijawab 400 dengan pesan yang bisa
/// dibaca, bukan diam-diam menghasilkan laporan kosong.
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
    fn menjadi_filter(self) -> AppResult<(SalesFilter, i64)> {
        let filter = SalesFilter {
            period: satuan_periode(self.period.as_deref())?,
            payment_method: pilihan(
                self.payment_method,
                &["cash", "transfer", "ewallet"],
                "Metode pembayaran",
            )?,
            transaction_type: pilihan(
                self.transaction_type,
                &["walk_in", "pre_order"],
                "Jenis transaksi",
            )?,
        };

        let limit = self
            .sales_limit
            .unwrap_or(PENJUALAN_BAKU)
            .clamp(1, PENJUALAN_MAKS);

        Ok((filter, limit))
    }
}

// ---------------------------------------------------------------------
// Dasbor
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct Dashboard {
    today_revenue: rust_decimal::Decimal,
    month_revenue: rust_decimal::Decimal,
    today_transaction_count: i64,
    product_count: i64,
    customer_count: i64,
    low_stock_count: i64,
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

    // Daftar penjualan terakhir sengaja tanpa batas periode: dasbor yang
    // kosong sepanjang pagi karena belum ada transaksi hari ini tidak
    // memberi tahu apa pun.
    let recent_sales =
        repo::recent_sales(&state.pool, &SalesFilter::periode(None), DASBOR_DAFTAR).await?;
    let low_stock_products = repo::low_stock(&state.pool, DASBOR_DAFTAR).await?;

    Ok(Json(Dashboard {
        today_revenue: omzet.today_revenue,
        month_revenue: omzet.month_revenue,
        today_transaction_count: omzet.today_transaction_count,
        product_count,
        customer_count,
        low_stock_count,
        recent_sales,
        low_stock_products,
    }))
}

// ---------------------------------------------------------------------
// Laporan penjualan
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SalesReport {
    summary: SalesSummary,
    trend: Vec<MonthlyPoint>,
    expense_breakdown: Vec<ExpenseSlice>,
    top_products: Vec<TopProduct>,
    sales: Vec<SaleRow>,
    /// Banyaknya baris yang terbawa di `sales`, dibandingkan batas yang
    /// diminta. Halaman memakainya untuk mengatakan bahwa daftarnya
    /// dipotong, bukan bahwa hanya segitu penjualannya.
    sales_limit: i64,
}

async fn sales_report(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ReportQuery>,
) -> AppResult<Json<SalesReport>> {
    user.require(&[Role::Owner])?;

    let (filter, sales_limit) = q.menjadi_filter()?;

    Ok(Json(SalesReport {
        summary: repo::sales_summary(&state.pool, &filter).await?,
        trend: repo::monthly_trend(&state.pool, &filter).await?,
        expense_breakdown: repo::expense_breakdown(&state.pool, filter.period).await?,
        top_products: repo::top_products(&state.pool, &filter, TERLARIS).await?,
        sales: repo::recent_sales(&state.pool, &filter, sales_limit).await?,
        sales_limit,
    }))
}
