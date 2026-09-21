//! Query agregat untuk dasbor dan laporan.
//!
//! Semua query penjualan berbagi saringan yang sama -- periode, metode
//! bayar, jenis transaksi -- dan hanya menghitung transaksi `completed`:
//! transaksi yang dibatalkan bukan omzet.
//!
//! Saringannya ditulis ulang di tiap query, bukan dirangkai dari potongan
//! string. `sqlx::query!` memeriksa SQL ke database saat kompilasi, dan itu
//! hanya bisa dilakukan kalau SQL-nya literal. Pengulangan ini harganya, dan
//! yang dibeli adalah kolom salah ketik yang ketahuan saat `cargo build`,
//! bukan saat pemilik toko membuka laporan.
//!
//! OMZET ADALAH `subtotal`, BUKAN `total_amount`. Sejak migrasi 0007 ongkir
//! punya kolom sendiri dan ikut tertambah di `total_amount`; ongkir bukan
//! barang dan tidak punya margin, jadi memasukkannya ke omzet membuat setiap
//! perhitungan laba salah.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Saringan bersama seluruh angka penjualan.
#[derive(Debug, Default)]
pub struct SalesFilter {
    /// Satuan untuk `date_trunc`: `day`, `week`, `month`, atau `year`.
    /// `None` berarti seluruh waktu.
    pub period: Option<&'static str>,
    pub payment_method: Option<String>,
    /// `walk_in` atau `pre_order`.
    pub transaction_type: Option<String>,
}

impl SalesFilter {
    /// Saringan yang hanya membatasi periode. Dipakai dasbor, yang tidak
    /// punya pilihan metode bayar maupun jenis transaksi.
    pub fn periode(period: Option<&'static str>) -> Self {
        Self {
            period,
            ..Self::default()
        }
    }
}

/// Angka pokok satu periode. Beban dan harga pokok ikut di sini supaya
/// laba bisa dihitung tanpa pemanggil perlu merangkainya sendiri.
#[derive(Debug, Serialize)]
pub struct SalesSummary {
    /// Omzet barang: jumlah `subtotal`, tanpa ongkir.
    pub revenue: Decimal,
    /// Ongkir yang ditagihkan ke pembeli. Diperlihatkan terpisah supaya
    /// jelas bahwa uang ini bukan hasil penjualan barang.
    pub shipping: Decimal,
    /// Harga pokok barang terjual, diambil dari harga beli batch yang benar-
    /// benar keluar. Batch tanpa harga beli jatuh ke `products.cost_price`
    /// peninggalan data lama, dan kalau keduanya kosong dihitung nol --
    /// lihat `items_without_cost`.
    pub cogs: Decimal,
    pub expenses: Decimal,
    /// `revenue - cogs - expenses`.
    pub profit: Decimal,
    pub transaction_count: i64,
    /// Banyaknya baris keluaran stok yang harga pokoknya belum diisi. Selama angka
    /// ini bukan nol, `cogs` dan `profit` adalah batas atas, bukan nilai
    /// sebenarnya -- dan halaman laporan mengatakannya.
    pub items_without_cost: i64,
}

pub async fn sales_summary(pool: &PgPool, filter: &SalesFilter) -> AppResult<SalesSummary> {
    let penjualan = sqlx::query!(
        r#"
        SELECT COALESCE(SUM(t.subtotal), 0)      AS "revenue!",
               COALESCE(SUM(t.shipping_cost), 0) AS "shipping!",
               COUNT(*)                          AS "count!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND ($1::text IS NULL
               OR t.created_at >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                  AT TIME ZONE 'Asia/Jakarta')
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        "#,
        filter.period,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    let pokok = sqlx::query!(
        r#"
        SELECT COALESCE(
                   SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)),
                   0
               ) AS "cogs!",
               COUNT(*) FILTER (
                   WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL
               ) AS "tanpa_pokok!"
        FROM stock_adjustments sa
        JOIN transactions t ON t.id = sa.reference_id
        LEFT JOIN product_batches pb ON pb.id = sa.batch_id
        LEFT JOIN products p ON p.id = sa.product_id
        WHERE sa.reason = 'sale'
          AND sa.change_qty < 0
          AND sa.reference_type = 'transaction'
          AND t.status = 'completed'
          AND ($1::text IS NULL
               OR t.created_at >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                  AT TIME ZONE 'Asia/Jakarta')
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        "#,
        filter.period,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    // Beban tidak mengenal metode bayar maupun jenis transaksi -- yang
    // berlaku padanya hanya periode.
    let beban = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(e.amount), 0) AS "total!"
        FROM expenses e
        WHERE $1::text IS NULL
           OR e.expense_date >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')::date
        "#,
        filter.period
    )
    .fetch_one(pool)
    .await?;

    Ok(SalesSummary {
        revenue: penjualan.revenue,
        shipping: penjualan.shipping,
        cogs: pokok.cogs,
        expenses: beban,
        profit: penjualan.revenue - pokok.cogs - beban,
        transaction_count: penjualan.count,
        items_without_cost: pokok.tanpa_pokok,
    })
}

/// Omzet hari ini dan bulan ini, keduanya pada zona toko.
#[derive(Debug, Serialize)]
pub struct TodayAndMonth {
    pub today_revenue: Decimal,
    pub month_revenue: Decimal,
    pub today_transaction_count: i64,
}

/// Satu query untuk dua angka: batas bulan sudah mencakup batas hari, jadi
/// `FILTER` cukup untuk memisahkan keduanya tanpa membaca tabel dua kali.
pub async fn today_and_month(pool: &PgPool) -> AppResult<TodayAndMonth> {
    let row = sqlx::query!(
        r#"
        SELECT COALESCE(SUM(t.subtotal) FILTER (
                   WHERE t.created_at >= date_trunc('day', now() AT TIME ZONE 'Asia/Jakarta')
                                         AT TIME ZONE 'Asia/Jakarta'), 0) AS "today!",
               COALESCE(SUM(t.subtotal), 0)                               AS "month!",
               COUNT(*) FILTER (
                   WHERE t.created_at >= date_trunc('day', now() AT TIME ZONE 'Asia/Jakarta')
                                         AT TIME ZONE 'Asia/Jakarta')     AS "today_count!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND t.created_at >= date_trunc('month', now() AT TIME ZONE 'Asia/Jakarta')
                              AT TIME ZONE 'Asia/Jakarta'
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(TodayAndMonth {
        today_revenue: row.today,
        month_revenue: row.month,
        today_transaction_count: row.today_count,
    })
}

/// Satu bulan pada grafik tren.
#[derive(Debug, Serialize)]
pub struct MonthlyPoint {
    /// `YYYY-MM`, dihitung pada zona toko.
    pub month: String,
    pub revenue: Decimal,
    pub expenses: Decimal,
}

/// Omzet dan beban enam bulan terakhir, termasuk bulan yang kosong.
///
/// Deret bulannya dibuat Postgres dengan `generate_series`, bukan dirakit di
/// Rust: kalau Rust yang menentukan "bulan ini", zona waktunya harus
/// ditiru di dua tempat dan cepat atau lambat keduanya akan berbeda.
///
/// Saringan periode sengaja tidak ikut -- grafik ini memang selalu enam
/// bulan. Saringan metode bayar dan jenis transaksi tetap berlaku supaya
/// grafik bercerita tentang irisan data yang sama dengan kartu di atasnya.
pub async fn monthly_trend(pool: &PgPool, filter: &SalesFilter) -> AppResult<Vec<MonthlyPoint>> {
    let rows = sqlx::query!(
        r#"
        WITH bulan AS (
            SELECT generate_series(
                date_trunc('month', now() AT TIME ZONE 'Asia/Jakarta') - INTERVAL '5 months',
                date_trunc('month', now() AT TIME ZONE 'Asia/Jakarta'),
                INTERVAL '1 month'
            ) AS awal
        )
        SELECT to_char(b.awal, 'YYYY-MM') AS "month!",
               COALESCE((
                   SELECT SUM(t.subtotal)
                   FROM transactions t
                   WHERE t.status = 'completed'
                     AND t.created_at AT TIME ZONE 'Asia/Jakarta' >= b.awal
                     AND t.created_at AT TIME ZONE 'Asia/Jakarta' < b.awal + INTERVAL '1 month'
                     AND ($1::text IS NULL OR t.payment_method = $1)
                     AND ($2::text IS NULL OR t.type = $2)
               ), 0) AS "revenue!",
               COALESCE((
                   SELECT SUM(e.amount)
                   FROM expenses e
                   WHERE e.expense_date >= b.awal::date
                     AND e.expense_date < (b.awal + INTERVAL '1 month')::date
               ), 0) AS "expenses!"
        FROM bulan b
        ORDER BY b.awal
        "#,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| MonthlyPoint {
            month: r.month,
            revenue: r.revenue,
            expenses: r.expenses,
        })
        .collect())
}

/// Satu kategori beban.
#[derive(Debug, Serialize)]
pub struct ExpenseSlice {
    pub category: String,
    pub amount: Decimal,
}

pub async fn expense_breakdown(
    pool: &PgPool,
    period: Option<&'static str>,
) -> AppResult<Vec<ExpenseSlice>> {
    let rows = sqlx::query!(
        r#"
        SELECT COALESCE(NULLIF(trim(e.category), ''), 'Lainnya') AS "category!",
               SUM(e.amount) AS "amount!"
        FROM expenses e
        WHERE $1::text IS NULL
           OR e.expense_date >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')::date
        GROUP BY 1
        ORDER BY 2 DESC
        "#,
        period
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ExpenseSlice {
            category: r.category,
            amount: r.amount,
        })
        .collect())
}

/// Satu baris tabel produk terlaris.
#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_id: Uuid,
    /// Nama saat terjual, bukan nama sekarang: produk yang sudah diganti
    /// namanya tetap dikenali pada laporan bulan lalu.
    pub name: String,
    pub sku: Option<String>,
    pub qty: i64,
    pub revenue: Decimal,
}

pub async fn top_products(
    pool: &PgPool,
    filter: &SalesFilter,
    limit: i64,
) -> AppResult<Vec<TopProduct>> {
    let rows = sqlx::query!(
        r#"
        SELECT ti.product_id                        AS "product_id!",
               max(ti.product_name_snapshot)        AS "name!",
               max(p.sku)                           AS sku,
               SUM(ti.qty)::bigint                  AS "qty!",
               SUM(ti.subtotal)                     AS "revenue!"
        FROM transaction_items ti
        JOIN transactions t ON t.id = ti.transaction_id
        LEFT JOIN products p ON p.id = ti.product_id
        WHERE t.status = 'completed'
          AND ($1::text IS NULL
               OR t.created_at >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                  AT TIME ZONE 'Asia/Jakarta')
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        GROUP BY ti.product_id
        ORDER BY "qty!" DESC, "revenue!" DESC
        LIMIT $4
        "#,
        filter.period,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref(),
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| TopProduct {
            product_id: r.product_id,
            name: r.name,
            sku: r.sku,
            qty: r.qty,
            revenue: r.revenue,
        })
        .collect())
}

/// Satu baris daftar penjualan. Bentuk ringkas: cukup untuk tabel dan
/// ekspor, tanpa menarik seluruh item tiap transaksi.
#[derive(Debug, Serialize)]
pub struct SaleRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    /// `null` untuk pembeli yang tidak dicatat namanya di kasir.
    pub customer_name: Option<String>,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub payment_method: String,
    pub item_count: i64,
    pub revenue: Decimal,
    pub shipping: Decimal,
    pub total_amount: Decimal,
}

pub async fn recent_sales(
    pool: &PgPool,
    filter: &SalesFilter,
    limit: i64,
) -> AppResult<Vec<SaleRow>> {
    let rows = sqlx::query!(
        r#"
        SELECT t.id                    AS "id!",
               t.created_at            AS "created_at!",
               c.name                  AS customer_name,
               t.type                  AS "transaction_type!",
               t.payment_method        AS "payment_method!",
               t.subtotal              AS "revenue!",
               t.shipping_cost         AS "shipping!",
               t.total_amount          AS "total_amount!",
               (SELECT count(*) FROM transaction_items ti WHERE ti.transaction_id = t.id)
                                       AS "item_count!"
        FROM transactions t
        LEFT JOIN customers c ON c.id = t.customer_id
        WHERE t.status = 'completed'
          AND ($1::text IS NULL
               OR t.created_at >= date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                  AT TIME ZONE 'Asia/Jakarta')
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        ORDER BY t.created_at DESC
        LIMIT $4
        "#,
        filter.period,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref(),
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SaleRow {
            id: r.id,
            created_at: r.created_at,
            customer_name: r.customer_name,
            transaction_type: r.transaction_type,
            payment_method: r.payment_method,
            item_count: r.item_count,
            revenue: r.revenue,
            shipping: r.shipping,
            total_amount: r.total_amount,
        })
        .collect())
}

/// Produk yang stoknya sudah menyentuh atau melewati ambangnya.
#[derive(Debug, Serialize)]
pub struct LowStockProduct {
    pub id: Uuid,
    pub name: String,
    pub sku: Option<String>,
    pub stock_qty: i32,
    pub low_stock_threshold: i32,
}

/// Peringatan stok menipis, yang paling tipis lebih dulu.
///
/// Hanya produk yang benar-benar dijual: induk yang punya varian tidak
/// memegang stok sendiri, jadi angka nol di barisnya bukan peringatan
/// tentang apa pun.
pub async fn low_stock(pool: &PgPool, limit: i64) -> AppResult<Vec<LowStockProduct>> {
    let rows = sqlx::query_as!(
        LowStockProduct,
        r#"
        SELECT p.id, p.name, p.sku, p.stock_qty, p.low_stock_threshold
        FROM products p
        WHERE p.is_active
          AND p.low_stock_threshold >= p.stock_qty
          AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)
        ORDER BY p.stock_qty - p.low_stock_threshold, p.name
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Cacahan yang tampil sebagai kartu di dasbor.
#[derive(Debug, Serialize)]
pub struct Counts {
    pub product_count: i64,
    pub customer_count: i64,
    pub low_stock_count: i64,
}

pub async fn counts(pool: &PgPool) -> AppResult<Counts> {
    let row = sqlx::query!(
        r#"
        SELECT (SELECT count(*) FROM products p
                WHERE p.is_active
                  AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id))
                   AS "product_count!",
               (SELECT count(*) FROM customers) AS "customer_count!",
               (SELECT count(*) FROM products p
                WHERE p.is_active
                  AND p.low_stock_threshold >= p.stock_qty
                  AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id))
                   AS "low_stock_count!"
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(Counts {
        product_count: row.product_count,
        customer_count: row.customer_count,
        low_stock_count: row.low_stock_count,
    })
}
