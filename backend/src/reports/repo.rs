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
//! OMZET ADALAH `subtotal - discount_amount`, BUKAN `total_amount`. Sejak
//! migrasi 0007 ongkir punya kolom sendiri dan ikut tertambah di
//! `total_amount`; ongkir bukan barang dan tidak punya margin, jadi
//! memasukkannya ke omzet membuat setiap perhitungan laba salah. Sejak
//! migrasi 0015 diskon juga punya kolom sendiri, dan ia kebalikannya: uang
//! yang TIDAK pernah diterima, jadi mengabaikannya membuat omzet terlalu
//! besar dan laba yang dilaporkan tidak pernah tercapai.
//!
//! OMZET SHOPEE TIDAK SEPENUHNYA CAIR. Shopee memotong `commission_fee`
//! (persentase BISA DIEDIT kasir) + `service_fee` (persentase BISA DIEDIT,
//! program opsional) + `withholding_tax` 0,5% (tetap) dari harga jualnya
//! sendiri, ditambah `seller_order_processing_fee` Rp1.250 per PESANAN
//! (bukan per barang), sebelum uangnya masuk ke toko. Nama field mengikuti
//! `v2.payment.get_escrow_detail` Shopee (lihat migrasi 0018), bukan istilah
//! rakitan sendiri. `revenue` tetap angka kotor -- sama dengan yang tercatat
//! di struk -- tapi `profit` mengurangi potongan itu lewat `platform_fees`,
//! supaya laba yang dilaporkan tidak lebih besar dari uang yang benar-benar
//! diterima. Kanal toko dan Tokopedia/TikTok tidak kena potongan ini.
//!
//! Sejak migrasi 0017 nilainya DIBACA, bukan dihitung ulang: setiap transaksi
//! Shopee menyimpan sendiri `platform_commission_fee`, `platform_service_fee`,
//! `platform_withholding_tax`, dan `platform_order_processing_fee` -- angka
//! yang sungguh dipakai saat checkout, termasuk kalau kasir mengedit persen
//! komisi/layanannya. Menjumlahkan tarif tetap di sini lagi akan salah tepat
//! untuk transaksi yang persennya diedit.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Saringan bersama seluruh angka penjualan.
#[derive(Debug, Default, Clone)]
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
    /// Omzet barang: jumlah `subtotal` sesudah diskon, tanpa ongkir. Angka
    /// KOTOR -- sebelum potongan platform Shopee, sama seperti yang tercatat
    /// di setiap transaksi. Lihat `platform_fees` untuk bagian yang tidak
    /// pernah benar-benar cair ke toko.
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
    /// Jumlah `platform_commission_fee + platform_service_fee +
    /// platform_withholding_tax + platform_order_processing_fee` tiap
    /// transaksi pada periode ini -- dibaca langsung dari kolom yang
    /// tersimpan saat checkout (migrasi 0017/0018), bukan dihitung ulang
    /// dengan tarif tetap. Nol untuk kanal selain Shopee dan nol kalau tidak
    /// ada penjualan Shopee pada periode ini.
    pub platform_fees: Decimal,
    /// `revenue - cogs - expenses - platform_fees`.
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
        SELECT COALESCE(SUM(t.subtotal - t.discount_amount), 0) AS "revenue!",
               COALESCE(SUM(t.shipping_cost), 0) AS "shipping!",
               COUNT(*)                          AS "count!",
               COALESCE(
                   SUM(t.platform_commission_fee + t.platform_service_fee
                       + t.platform_withholding_tax + t.platform_order_processing_fee),
                   0
               )                                 AS "platform_fees!"
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
        platform_fees: penjualan.platform_fees,
        profit: penjualan.revenue - pokok.cogs - beban - penjualan.platform_fees,
        transaction_count: penjualan.count,
        items_without_cost: pokok.tanpa_pokok,
    })
}

/// Angka periode SEBELUMNYA, sepanjang periode yang sama, untuk kartu trend
/// di halaman laporan. `None` kalau saringan "Seluruh Waktu" -- rentang itu
/// tidak punya periode sebelumnya untuk dibandingkan.
///
/// Jendelanya `[date_trunc(period, now) - '1 <unit>', date_trunc(period,
/// now))`, digeser satu unit periode ke belakang dari jendela
/// `sales_summary`. Query di bawah meniru `sales_summary` baris demi baris
/// -- satu-satunya beda adalah batas waktu closed-open ini menggantikan
/// `>= date_trunc(...)` yang terbuka ke `now()`.
///
/// `'1 ' || $1` aman sebagai literal interval di sini karena `$1` cuma
/// pernah salah satu dari `day/week/month/year` yang sudah divalidasi
/// `satuan_periode()` di `routes.rs` -- tidak pernah teks bebas dari
/// pengguna.
pub async fn sales_summary_previous(
    pool: &PgPool,
    filter: &SalesFilter,
) -> AppResult<Option<SalesSummary>> {
    let Some(period) = filter.period else {
        return Ok(None);
    };

    let penjualan = sqlx::query!(
        r#"
        SELECT COALESCE(SUM(t.subtotal - t.discount_amount), 0) AS "revenue!",
               COALESCE(SUM(t.shipping_cost), 0) AS "shipping!",
               COUNT(*)                          AS "count!",
               COALESCE(
                   SUM(t.platform_commission_fee + t.platform_service_fee
                       + t.platform_withholding_tax + t.platform_order_processing_fee),
                   0
               )                                 AS "platform_fees!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND t.created_at >= (date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                - ('1 ' || $1)::interval) AT TIME ZONE 'Asia/Jakarta'
          AND t.created_at <  date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                               AT TIME ZONE 'Asia/Jakarta'
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        "#,
        period,
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
          AND t.created_at >= (date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                - ('1 ' || $1)::interval) AT TIME ZONE 'Asia/Jakarta'
          AND t.created_at <  date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                               AT TIME ZONE 'Asia/Jakarta'
          AND ($2::text IS NULL OR t.payment_method = $2)
          AND ($3::text IS NULL OR t.type = $3)
        "#,
        period,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    let beban = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(e.amount), 0) AS "total!"
        FROM expenses e
        WHERE e.expense_date >= (date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')
                                  - ('1 ' || $1)::interval)::date
          AND e.expense_date <  date_trunc($1, now() AT TIME ZONE 'Asia/Jakarta')::date
        "#,
        period
    )
    .fetch_one(pool)
    .await?;

    Ok(Some(SalesSummary {
        revenue: penjualan.revenue,
        shipping: penjualan.shipping,
        cogs: pokok.cogs,
        expenses: beban,
        platform_fees: penjualan.platform_fees,
        profit: penjualan.revenue - pokok.cogs - beban - penjualan.platform_fees,
        transaction_count: penjualan.count,
        items_without_cost: pokok.tanpa_pokok,
    }))
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
        SELECT COALESCE(SUM(t.subtotal - t.discount_amount) FILTER (
                   WHERE t.created_at >= date_trunc('day', now() AT TIME ZONE 'Asia/Jakarta')
                                         AT TIME ZONE 'Asia/Jakarta'), 0) AS "today!",
               COALESCE(SUM(t.subtotal - t.discount_amount), 0)           AS "month!",
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
                   SELECT SUM(t.subtotal - t.discount_amount)
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
///
/// `revenue` di sini harga barangnya SEBELUM diskon. Diskon melekat pada
/// transaksi, bukan pada barang tertentu, jadi membagikannya ke tiap baris
/// berarti mengarang angka yang tidak pernah ada di struk mana pun. Yang
/// dijawab tabel ini adalah "barang mana yang paling laku", dan untuk
/// pertanyaan itu harga jualnya yang relevan -- bukan potongan yang diberikan
/// kepada pembelinya.
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
               t.subtotal - t.discount_amount AS "revenue!",
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

/// Satu baris item pada detail transaksi.
#[derive(Debug, Serialize)]
pub struct TransactionDetailItem {
    pub id: Uuid,
    pub product_id: Uuid,
    /// Nama saat terjual, bukan nama sekarang -- lihat `TopProduct::name`.
    pub name: String,
    /// SKU produk SAAT INI: item transaksi tidak menyimpan SKU sendiri,
    /// jadi ini dibaca dari `products` dan `null` kalau produknya sudah
    /// dihapus.
    pub sku: Option<String>,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

/// Detail lengkap satu transaksi untuk halaman `/laporan/transaksi/{id}`.
/// Owner saja -- sama seperti `sales_summary`, karena memuat laba dan
/// potongan Shopee.
#[derive(Debug, Serialize)]
pub struct TransactionDetail {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub voided_at: Option<DateTime<Utc>>,
    pub void_reason: Option<String>,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub sales_channel: String,
    pub payment_method: String,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_email: Option<String>,
    pub cashier_name: String,
    pub subtotal: Decimal,
    pub discount_amount: Decimal,
    pub shipping_cost: Decimal,
    pub total_amount: Decimal,
    pub amount_paid: Option<Decimal>,
    pub change_amount: Option<Decimal>,
    pub platform_commission_fee_percent: Decimal,
    pub platform_commission_fee: Decimal,
    pub platform_service_fee_percent: Decimal,
    pub platform_service_fee: Decimal,
    pub platform_withholding_tax: Decimal,
    pub platform_order_processing_fee: Decimal,
    /// Harga pokok baris yang keluar untuk transaksi ini. Batas atas kalau
    /// `items_without_cost > 0` -- lihat `SalesSummary::cogs`.
    pub cogs: Decimal,
    pub items_without_cost: i64,
    /// `subtotal - discount_amount - cogs - (platform_commission_fee +
    /// platform_service_fee + platform_withholding_tax +
    /// platform_order_processing_fee)`. Beban toko sengaja tidak ikut --
    /// itu milik periode, bukan transaksi tunggal.
    pub net_profit: Decimal,
    pub items: Vec<TransactionDetailItem>,
}

pub async fn transaction_detail(pool: &PgPool, id: Uuid) -> AppResult<Option<TransactionDetail>> {
    let Some(head) = sqlx::query!(
        r#"
        SELECT t.id                          AS "id!",
               t.created_at                  AS "created_at!",
               t.status                      AS "status!",
               t.voided_at,
               t.void_reason,
               t.type                        AS "transaction_type!",
               t.sales_channel               AS "sales_channel!",
               t.payment_method               AS "payment_method!",
               c.name                        AS customer_name,
               c.phone                       AS customer_phone,
               c.email                       AS customer_email,
               u.name                        AS "cashier_name!",
               t.subtotal                    AS "subtotal!",
               t.discount_amount             AS "discount_amount!",
               t.shipping_cost               AS "shipping_cost!",
               t.total_amount                AS "total_amount!",
               t.amount_paid,
               t.change_amount,
               t.platform_commission_fee_percent AS "platform_commission_fee_percent!",
               t.platform_commission_fee         AS "platform_commission_fee!",
               t.platform_service_fee_percent    AS "platform_service_fee_percent!",
               t.platform_service_fee            AS "platform_service_fee!",
               t.platform_withholding_tax        AS "platform_withholding_tax!",
               t.platform_order_processing_fee   AS "platform_order_processing_fee!"
        FROM transactions t
        LEFT JOIN customers c ON c.id = t.customer_id
        JOIN users u ON u.id = t.cashier_user_id
        WHERE t.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let items = sqlx::query_as!(
        TransactionDetailItem,
        r#"
        SELECT ti.id                    AS "id!",
               ti.product_id             AS "product_id!",
               ti.product_name_snapshot AS "name!",
               p.sku,
               ti.qty                    AS "qty!",
               ti.unit_price             AS "unit_price!",
               ti.subtotal               AS "subtotal!"
        FROM transaction_items ti
        LEFT JOIN products p ON p.id = ti.product_id
        WHERE ti.transaction_id = $1
        ORDER BY ti.created_at, ti.id
        "#,
        id
    )
    .fetch_all(pool)
    .await?;

    // Sama persis logikanya dengan query `pokok` di `sales_summary`, cuma
    // disaring ke satu transaksi lewat `reference_id` alih-alih rentang
    // periode.
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
        LEFT JOIN product_batches pb ON pb.id = sa.batch_id
        LEFT JOIN products p ON p.id = sa.product_id
        WHERE sa.reason = 'sale'
          AND sa.change_qty < 0
          AND sa.reference_type = 'transaction'
          AND sa.reference_id = $1
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    let platform_fees = head.platform_commission_fee
        + head.platform_service_fee
        + head.platform_withholding_tax
        + head.platform_order_processing_fee;

    Ok(Some(TransactionDetail {
        id: head.id,
        created_at: head.created_at,
        status: head.status,
        voided_at: head.voided_at,
        void_reason: head.void_reason,
        transaction_type: head.transaction_type,
        sales_channel: head.sales_channel,
        payment_method: head.payment_method,
        customer_name: head.customer_name,
        customer_phone: head.customer_phone,
        customer_email: head.customer_email,
        cashier_name: head.cashier_name,
        subtotal: head.subtotal,
        discount_amount: head.discount_amount,
        shipping_cost: head.shipping_cost,
        total_amount: head.total_amount,
        amount_paid: head.amount_paid,
        change_amount: head.change_amount,
        platform_commission_fee_percent: head.platform_commission_fee_percent,
        platform_commission_fee: head.platform_commission_fee,
        platform_service_fee_percent: head.platform_service_fee_percent,
        platform_service_fee: head.platform_service_fee,
        platform_withholding_tax: head.platform_withholding_tax,
        platform_order_processing_fee: head.platform_order_processing_fee,
        cogs: pokok.cogs,
        items_without_cost: pokok.tanpa_pokok,
        net_profit: head.subtotal - head.discount_amount - pokok.cogs - platform_fees,
        items,
    }))
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

/// Nilai persediaan saat ini: sisa stok tiap batch dikali harga belinya.
#[derive(Debug, Serialize)]
pub struct StockValue {
    pub value: Decimal,
    /// Batch yang masih punya sisa tetapi harga belinya kosong di batch
    /// maupun produk. Dihitung nol, jadi selama bukan nol `value` adalah
    /// batas bawah.
    pub batches_without_cost: i64,
}

/// Potret stok hari ini -- tidak ikut saringan periode. Harga beli dibaca
/// dengan urutan yang sama seperti harga pokok di `sales_summary`: harga
/// batch, lalu `products.cost_price` peninggalan data lama.
///
/// Induk yang punya varian dilewati: stok sungguhan ada di varian, dan
/// batch induk tidak ada.
pub async fn stock_value(pool: &PgPool) -> AppResult<StockValue> {
    let row = sqlx::query!(
        r#"
        SELECT COALESCE(
                   SUM(pb.remaining_qty * COALESCE(pb.purchase_price, p.cost_price, 0)),
                   0
               ) AS "value!",
               COUNT(*) FILTER (
                   WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL
               ) AS "tanpa_pokok!"
        FROM product_batches pb
        JOIN products p ON p.id = pb.product_id
        WHERE p.is_active
          AND pb.remaining_qty > 0
          AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(StockValue {
        value: row.value,
        batches_without_cost: row.tanpa_pokok,
    })
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
