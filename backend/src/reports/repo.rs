//! Query agregat laporan: hanya transaksi `completed`; omzet = `subtotal - discount_amount` (bukan `total_amount`); `profit` mengurangi `platform_fees` Shopee dari kolom tersimpan (migrasi 0017/0018).

use crate::error::AppResult;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Tanggal hari ini pada zona toko; WIB tanpa DST, jadi offset tetap cukup.
pub fn hari_ini() -> NaiveDate {
    let jakarta = FixedOffset::east_opt(7 * 3600).expect("offset WIB sah");
    Utc::now().with_timezone(&jakarta).date_naive()
}

/// Satuan periode baku yang berjalan dari awal satuan itu sampai sekarang.
#[derive(Debug, Clone, Copy)]
pub enum Periode {
    Hari,
    /// Pekan mulai Senin, sama dengan `date_trunc('week')`.
    Pekan,
    Bulan,
    Tahun,
}

/// Rentang waktu penjualan; tanggal selalu kalender zona toko dan batas akhir ikut dihitung.
#[derive(Debug, Default, Clone)]
pub enum Rentang {
    /// Seluruh waktu.
    #[default]
    Semua,
    Periode(Periode),
    /// Tanggal pilihan; sisi yang kosong tak dibatasi.
    Tanggal {
        dari: Option<NaiveDate>,
        sampai: Option<NaiveDate>,
    },
}

impl Rentang {
    /// Batas tanggal (awal, akhir) untuk query; `None` berarti sisi itu tak dibatasi.
    pub fn jendela(&self) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self {
            Rentang::Semua => (None, None),
            Rentang::Periode(p) => (Some(awal_periode(*p, hari_ini())), None),
            Rentang::Tanggal { dari, sampai } => (*dari, *sampai),
        }
    }
}

fn awal_periode(periode: Periode, hari: NaiveDate) -> NaiveDate {
    match periode {
        Periode::Hari => hari,
        Periode::Pekan => hari - Duration::days(i64::from(hari.weekday().num_days_from_monday())),
        Periode::Bulan => hari.with_day(1).expect("tanggal 1 selalu ada"),
        Periode::Tahun => hari.with_ordinal(1).expect("hari ke-1 selalu ada"),
    }
}

/// Saringan bersama seluruh angka penjualan.
#[derive(Debug, Default, Clone)]
pub struct SalesFilter {
    pub rentang: Rentang,
    pub payment_method: Option<String>,
    /// `walk_in` atau `pre_order`.
    pub transaction_type: Option<String>,
}

impl SalesFilter {
    /// Saringan yang hanya membatasi periode, dipakai dasbor yang tak punya metode bayar/jenis transaksi.
    pub fn periode(periode: Periode) -> Self {
        Self {
            rentang: Rentang::Periode(periode),
            ..Self::default()
        }
    }

    pub fn jendela(&self) -> (Option<NaiveDate>, Option<NaiveDate>) {
        self.rentang.jendela()
    }

    /// Saringan yang sama untuk rentang pembanding tepat sebelumnya; `None` bila tak ada pembanding (seluruh waktu, atau tanggal pilihan yang salah satu sisinya kosong).
    pub fn sebelumnya(&self) -> Option<Self> {
        let (dari, sampai) = match &self.rentang {
            Rentang::Semua => return None,
            Rentang::Periode(p) => {
                let akhir = awal_periode(*p, hari_ini()).pred_opt()?;
                let awal = match p {
                    Periode::Hari => akhir,
                    Periode::Pekan => akhir - Duration::days(6),
                    Periode::Bulan => akhir.with_day(1)?,
                    Periode::Tahun => akhir.with_ordinal(1)?,
                };
                (awal, akhir)
            }
            Rentang::Tanggal {
                dari: Some(dari),
                sampai: Some(sampai),
            } => {
                let panjang = (*sampai - *dari).num_days() + 1;
                (*dari - Duration::days(panjang), dari.pred_opt()?)
            }
            Rentang::Tanggal { .. } => return None,
        };

        Some(Self {
            rentang: Rentang::Tanggal {
                dari: Some(dari),
                sampai: Some(sampai),
            },
            ..self.clone()
        })
    }
}

/// Angka pokok satu periode; beban dan harga pokok ikut agar laba dihitung tanpa perakitan pemanggil.
#[derive(Debug, Serialize)]
pub struct SalesSummary {
    /// Omzet barang: jumlah `subtotal` setelah diskon tanpa ongkir, angka kotor sebelum potongan Shopee (lihat `platform_fees`).
    pub revenue: Decimal,
    /// Ongkir yang ditagihkan ke pembeli, terpisah karena bukan hasil penjualan barang; ongkir yang ditanggung toko tak masuk sini (lihat `shipping_subsidy`).
    pub shipping: Decimal,
    /// Ongkir yang ditanggung toko lewat diskon ongkir (migrasi 0023); sudah termasuk di `expenses`, dipisah supaya halaman bisa menyebutnya.
    pub shipping_subsidy: Decimal,
    /// Berat barang terjual dalam kg (pack × ukuran produk); penjualan produk tanpa ukuran, atau yang produknya sudah dihapus, tak ikut dihitung.
    pub sold_kg: Decimal,
    /// Harga pokok barang terjual dari harga beli batch yang keluar; batch tanpa harga beli jatuh ke `products.cost_price` peninggalan lama, kosong dihitung nol (lihat `items_without_cost`).
    pub cogs: Decimal,
    /// Beban toko: tabel `expenses` ditambah `shipping_subsidy`.
    pub expenses: Decimal,
    /// Jumlah `platform_fee_total` (seluruh potongan Shopee dan Tokopedia plus PPh) periode ini dari kolom tersimpan (migrasi 0038); komisi, layanan, dan biaya proses nol di toko, PPh terisi di semua kanal.
    pub platform_fees: Decimal,
    /// Barang rusak, kedaluwarsa, atau hilang periode ini dinilai harga beli (lihat `kerugian_stok`); hanya periode yang menyaringnya, seperti tabel beban.
    pub stock_loss: Decimal,
    /// `revenue - cogs - expenses - platform_fees - stock_loss`.
    pub profit: Decimal,
    pub transaction_count: i64,
    /// Baris keluaran stok yang harga pokoknya belum diisi; selama bukan nol `cogs` dan `profit` adalah batas atas dan halaman mengatakannya.
    pub items_without_cost: i64,
}

pub async fn sales_summary(pool: &PgPool, filter: &SalesFilter) -> AppResult<SalesSummary> {
    let (dari, sampai) = filter.jendela();
    let penjualan = sqlx::query!(
        r#"
        SELECT COALESCE(SUM(t.subtotal - t.discount_amount), 0) AS "revenue!",
               COALESCE(SUM(t.shipping_charged), 0) AS "shipping!",
               COALESCE(SUM(t.shipping_subsidy), 0) AS "shipping_subsidy!",
               COUNT(*)                          AS "count!",
               COALESCE(SUM(t.platform_fee_total), 0) AS "platform_fees!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    let pokok = harga_pokok(pool, filter).await?;

    let terjual = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(ti.qty * p.variant_size), 0) AS "kg!"
        FROM transaction_items ti
        JOIN transactions t ON t.id = ti.transaction_id
        JOIN products p ON p.id = ti.product_id
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    // Beban tak mengenal metode bayar maupun jenis transaksi, hanya periode.
    let beban = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(e.amount), 0) AS "total!"
        FROM expenses e
        WHERE ($1::date IS NULL OR e.expense_date >= $1)
          AND ($2::date IS NULL OR e.expense_date <= $2)
        "#,
        dari,
        sampai
    )
    .fetch_one(pool)
    .await?;

    let beban = beban + penjualan.shipping_subsidy;
    let rugi = kerugian_stok(pool, filter).await?.total;

    Ok(SalesSummary {
        revenue: penjualan.revenue,
        shipping: penjualan.shipping,
        shipping_subsidy: penjualan.shipping_subsidy,
        sold_kg: terjual,
        cogs: pokok.cogs,
        expenses: beban,
        platform_fees: penjualan.platform_fees,
        stock_loss: rugi,
        profit: penjualan.revenue - pokok.cogs - beban - penjualan.platform_fees - rugi,
        transaction_count: penjualan.count,
        items_without_cost: pokok.tanpa_pokok,
    })
}

/// Harga pokok barang terjual dan jumlah baris keluaran stok yang harga belinya kosong (dihitung nol).
struct HargaPokok {
    cogs: Decimal,
    tanpa_pokok: i64,
}

/// Satu-satunya hitungan harga pokok periode, dipakai ringkasan dan laba rugi agar keduanya tak bisa berselisih.
async fn harga_pokok(pool: &PgPool, filter: &SalesFilter) -> AppResult<HargaPokok> {
    let (dari, sampai) = filter.jendela();
    let row = sqlx::query!(
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
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    Ok(HargaPokok {
        cogs: row.cogs,
        tanpa_pokok: row.tanpa_pokok,
    })
}

/// Kerugian stok satu alasan: barang yang dikurangi manual karena rusak, kedaluwarsa, atau hilang, dinilai dengan harga beli batchnya.
#[derive(Debug, Serialize)]
pub struct KerugianStokSlice {
    /// `rusak`, `kedaluwarsa`, atau `hilang`.
    pub reason: String,
    pub qty: i64,
    pub amount: Decimal,
}

#[derive(Debug)]
struct KerugianStok {
    per_alasan: Vec<KerugianStokSlice>,
    total: Decimal,
    tanpa_pokok: i64,
}

/// Satu-satunya hitungan kerugian stok, dipakai ringkasan, laba rugi, dan tab Stok; hanya periode yang menyaring karena barang terbuang tak punya metode bayar, dan `salah_catat` serta koreksi tanpa alasan tak ikut.
async fn kerugian_stok(pool: &PgPool, filter: &SalesFilter) -> AppResult<KerugianStok> {
    let (dari, sampai) = filter.jendela();
    let rows = sqlx::query!(
        r#"
        SELECT sa.manual_reason AS "reason!",
               SUM(ABS(sa.change_qty))::bigint AS "qty!",
               COALESCE(SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)), 0) AS "amount!",
               COUNT(*) FILTER (WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL) AS "tanpa_pokok!"
        FROM stock_adjustments sa
        LEFT JOIN product_batches pb ON pb.id = sa.batch_id
        LEFT JOIN products p ON p.id = sa.product_id
        WHERE sa.manual_reason IN ('rusak', 'kedaluwarsa', 'hilang')
          AND ($1::date IS NULL
               OR sa.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR sa.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        GROUP BY sa.manual_reason
        ORDER BY 3 DESC
        "#,
        dari,
        sampai
    )
    .fetch_all(pool)
    .await?;

    let total = rows.iter().map(|r| r.amount).sum();
    let tanpa_pokok = rows.iter().map(|r| r.tanpa_pokok).sum();
    Ok(KerugianStok {
        per_alasan: rows
            .into_iter()
            .map(|r| KerugianStokSlice {
                reason: r.reason,
                qty: r.qty,
                amount: r.amount,
            })
            .collect(),
        total,
        tanpa_pokok,
    })
}

/// Laba rugi bertingkat (format UMKM) satu periode; `net_profit` sama dengan `SalesSummary::profit` dengan saringan yang sama.
#[derive(Debug, Serialize)]
pub struct LabaRugi {
    /// Jumlah `subtotal` sebelum diskon.
    pub gross_sales: Decimal,
    /// Diskon umum lama (tak lagi ditulis sejak migrasi 0023), tetap dilaporkan agar periode lama benar.
    pub discounts: Decimal,
    /// `gross_sales - discounts`, sama dengan omzet barang di seluruh laporan.
    pub net_sales: Decimal,
    pub cogs: Decimal,
    pub gross_profit: Decimal,
    pub commission_fee: Decimal,
    /// Komisi dinamis Tokopedia.
    pub dynamic_commission_fee: Decimal,
    /// Biaya layanan Shopee.
    pub service_fee: Decimal,
    /// Biaya layanan logistik Tokopedia.
    pub logistics_fee: Decimal,
    pub order_processing_fee: Decimal,
    pub shipping_subsidy: Decimal,
    /// Jumlah enam baris di atas: biaya yang timbul karena berjualan, di luar beban operasional.
    pub selling_costs: Decimal,
    /// Tabel `expenses` per kategori, tanpa ongkir ditanggung toko (sudah di `selling_costs`).
    pub operating_expenses: Vec<ExpenseSlice>,
    pub operating_expenses_total: Decimal,
    /// Barang dikurangi manual karena rusak, kedaluwarsa, atau hilang, dinilai harga beli; baris kerugian sendiri, bukan bagian harga pokok penjualan.
    pub stock_losses: Vec<KerugianStokSlice>,
    pub stock_loss_total: Decimal,
    /// Baris kerugian stok tanpa harga beli (dihitung nol); selama bukan nol kerugiannya batas bawah.
    pub stock_loss_without_cost: i64,
    pub profit_before_tax: Decimal,
    /// PPh final UMKM yang tercatat per transaksi di semua kanal.
    pub withholding_tax: Decimal,
    pub net_profit: Decimal,
    pub transaction_count: i64,
    /// Selama bukan nol, `cogs` terlalu kecil dan semua laba sesudahnya batas atas.
    pub items_without_cost: i64,
}

pub async fn laba_rugi(pool: &PgPool, filter: &SalesFilter) -> AppResult<LabaRugi> {
    let (dari, sampai) = filter.jendela();
    let jual = sqlx::query!(
        r#"
        SELECT COALESCE(SUM(t.subtotal), 0)                        AS "gross_sales!",
               COALESCE(SUM(t.discount_amount), 0)                 AS "discounts!",
               COALESCE(SUM(t.platform_commission_fee), 0)         AS "commission_fee!",
               COALESCE(SUM(t.platform_dynamic_commission_fee), 0) AS "dynamic_commission_fee!",
               COALESCE(SUM(t.platform_service_fee), 0)            AS "service_fee!",
               COALESCE(SUM(t.platform_logistics_fee), 0)          AS "logistics_fee!",
               COALESCE(SUM(t.platform_order_processing_fee), 0)   AS "order_processing_fee!",
               COALESCE(SUM(t.platform_withholding_tax), 0)        AS "withholding_tax!",
               COALESCE(SUM(t.shipping_subsidy), 0)                AS "shipping_subsidy!",
               COUNT(*)                                            AS "count!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    let pokok = harga_pokok(pool, filter).await?;

    let beban = sqlx::query!(
        r#"
        SELECT COALESCE(NULLIF(trim(e.category), ''), 'Lainnya') AS "category!",
               SUM(e.amount)                                     AS "amount!"
        FROM expenses e
        WHERE ($1::date IS NULL OR e.expense_date >= $1)
          AND ($2::date IS NULL OR e.expense_date <= $2)
        GROUP BY 1
        ORDER BY 2 DESC
        "#,
        dari,
        sampai
    )
    .fetch_all(pool)
    .await?;

    let operating_expenses: Vec<ExpenseSlice> = beban
        .into_iter()
        .map(|r| ExpenseSlice {
            category: r.category,
            amount: r.amount,
        })
        .collect();
    let operating_expenses_total: Decimal = operating_expenses.iter().map(|b| b.amount).sum();

    let net_sales = jual.gross_sales - jual.discounts;
    let gross_profit = net_sales - pokok.cogs;
    let selling_costs = jual.commission_fee
        + jual.dynamic_commission_fee
        + jual.service_fee
        + jual.logistics_fee
        + jual.order_processing_fee
        + jual.shipping_subsidy;
    let rugi = kerugian_stok(pool, filter).await?;
    let profit_before_tax = gross_profit - selling_costs - operating_expenses_total - rugi.total;

    Ok(LabaRugi {
        gross_sales: jual.gross_sales,
        discounts: jual.discounts,
        net_sales,
        cogs: pokok.cogs,
        gross_profit,
        commission_fee: jual.commission_fee,
        dynamic_commission_fee: jual.dynamic_commission_fee,
        service_fee: jual.service_fee,
        logistics_fee: jual.logistics_fee,
        order_processing_fee: jual.order_processing_fee,
        shipping_subsidy: jual.shipping_subsidy,
        selling_costs,
        operating_expenses,
        operating_expenses_total,
        stock_losses: rugi.per_alasan,
        stock_loss_total: rugi.total,
        stock_loss_without_cost: rugi.tanpa_pokok,
        profit_before_tax,
        withholding_tax: jual.withholding_tax,
        net_profit: profit_before_tax - jual.withholding_tax,
        transaction_count: jual.count,
        items_without_cost: pokok.tanpa_pokok,
    })
}

/// Kanal penjualan yang selalu tampil di laporan, dalam urutan tampil; harus sama dengan CHECK `transactions_sales_channel_check`.
const KANAL: [&str; 3] = ["toko", "shopee", "tiktok"];

/// Omzet dan laba satu kanal penjualan (`toko` = offline, `shopee`, `tiktok` = Tokopedia).
#[derive(Debug, Serialize)]
pub struct ChannelSummary {
    pub channel: String,
    pub revenue: Decimal,
    pub cogs: Decimal,
    /// Seluruh potongan platform (`platform_fee_total`); PPh juga terisi di kanal toko (migrasi 0035).
    pub platform_fees: Decimal,
    pub shipping_subsidy: Decimal,
    /// `revenue - cogs - platform_fees - shipping_subsidy`; tabel `expenses` tak ikut karena tak dicatat per kanal, jadi jumlah laba kanal lebih besar dari `SalesSummary::profit` sebesar beban itu.
    pub profit: Decimal,
    pub transaction_count: i64,
    /// Baris keluaran stok kanal ini tanpa harga pokok; selama bukan nol `cogs` dan `profit` kanal ini adalah batas atas.
    pub items_without_cost: i64,
}

/// Ringkasan per kanal dengan saringan yang sama seperti `sales_summary`; ketiga kanal selalu ada walau kosong supaya tata letak halaman tak berubah.
pub async fn channel_summaries(
    pool: &PgPool,
    filter: &SalesFilter,
) -> AppResult<Vec<ChannelSummary>> {
    let (dari, sampai) = filter.jendela();
    let penjualan = sqlx::query!(
        r#"
        SELECT t.sales_channel AS "channel!",
               COALESCE(SUM(t.subtotal - t.discount_amount), 0) AS "revenue!",
               COALESCE(SUM(t.shipping_subsidy), 0) AS "shipping_subsidy!",
               COUNT(*)                          AS "count!",
               COALESCE(SUM(t.platform_fee_total), 0) AS "platform_fees!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        GROUP BY t.sales_channel
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_all(pool)
    .await?;

    let pokok = sqlx::query!(
        r#"
        SELECT t.sales_channel AS "channel!",
               COALESCE(
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
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        GROUP BY t.sales_channel
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_all(pool)
    .await?;

    Ok(KANAL
        .iter()
        .map(|kanal| {
            let jual = penjualan.iter().find(|r| r.channel == *kanal);
            let harga_pokok = pokok.iter().find(|r| r.channel == *kanal);

            let revenue = jual.map(|r| r.revenue).unwrap_or_default();
            let platform_fees = jual.map(|r| r.platform_fees).unwrap_or_default();
            let shipping_subsidy = jual.map(|r| r.shipping_subsidy).unwrap_or_default();
            let cogs = harga_pokok.map(|r| r.cogs).unwrap_or_default();

            ChannelSummary {
                channel: (*kanal).to_string(),
                revenue,
                cogs,
                platform_fees,
                shipping_subsidy,
                profit: revenue - cogs - platform_fees - shipping_subsidy,
                transaction_count: jual.map(|r| r.count).unwrap_or_default(),
                items_without_cost: harga_pokok.map(|r| r.tanpa_pokok).unwrap_or_default(),
            }
        })
        .collect())
}

/// Periode pembanding untuk kartu trend (`None` bila tak ada pembanding, lihat `SalesFilter::sebelumnya`): angka yang sama dihitung pada rentang tepat sebelumnya.
pub async fn sales_summary_previous(
    pool: &PgPool,
    filter: &SalesFilter,
) -> AppResult<Option<SalesSummary>> {
    match filter.sebelumnya() {
        Some(sebelumnya) => Ok(Some(sales_summary(pool, &sebelumnya).await?)),
        None => Ok(None),
    }
}

/// Omzet hari ini dan bulan ini, keduanya pada zona toko.
#[derive(Debug, Serialize)]
pub struct TodayAndMonth {
    pub today_revenue: Decimal,
    pub month_revenue: Decimal,
    pub today_transaction_count: i64,
}

/// Satu query untuk dua angka: batas bulan mencakup batas hari sehingga `FILTER` memisahkan keduanya tanpa membaca tabel dua kali.
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

/// Butir waktu grafik dan tabel tren, dipilih dari panjang rentang agar banyaknya batang tetap terbaca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Butir {
    #[serde(rename = "hour")]
    Jam,
    #[serde(rename = "day")]
    Hari,
    #[serde(rename = "month")]
    Bulan,
}

impl Butir {
    /// Satu hari per jam, sampai dua bulan per hari, selebihnya per bulan.
    pub fn untuk(dari: NaiveDate, sampai: NaiveDate) -> Self {
        match (sampai - dari).num_days() + 1 {
            ..=1 => Self::Jam,
            2..=62 => Self::Hari,
            _ => Self::Bulan,
        }
    }

    /// Nama satuan untuk `date_trunc` dan `interval` Postgres.
    fn sql(self) -> &'static str {
        match self {
            Self::Jam => "hour",
            Self::Hari => "day",
            Self::Bulan => "month",
        }
    }

    /// Awal batang pertama dan batang terakhir; butir bulan dibulatkan ke tanggal 1 agar tiap batang satu bulan utuh.
    fn deret(self, dari: NaiveDate, sampai: NaiveDate) -> (NaiveDateTime, NaiveDateTime) {
        let tengah_malam = |d: NaiveDate| d.and_hms_opt(0, 0, 0).expect("00:00 selalu ada");
        match self {
            Self::Jam => (
                tengah_malam(dari),
                sampai.and_hms_opt(23, 0, 0).expect("23:00 selalu ada"),
            ),
            Self::Hari => (tengah_malam(dari), tengah_malam(sampai)),
            Self::Bulan => (
                tengah_malam(dari.with_day(1).expect("tanggal 1 selalu ada")),
                tengah_malam(sampai.with_day(1).expect("tanggal 1 selalu ada")),
            ),
        }
    }
}

/// Satu batang tren; `expenses` berisi tabel beban plus ongkir ditanggung toko seperti `SalesSummary`, kecuali butir jam yang hanya memuat ongkir karena beban dicatat per tanggal.
#[derive(Debug, Serialize)]
pub struct TitikTren {
    /// Awal batang pada zona toko, tanpa zona (`2026-11-14T00:00:00`).
    pub start: NaiveDateTime,
    pub revenue: Decimal,
    pub cogs: Decimal,
    pub platform_fees: Decimal,
    pub expenses: Decimal,
    /// Kerugian stok (lihat `kerugian_stok`) menurut waktu pencatatannya.
    pub stock_loss: Decimal,
    /// `revenue - cogs - platform_fees - expenses - stock_loss`; untuk butir hari dan bulan jumlah seluruh batang sama dengan `SalesSummary::profit`.
    pub profit: Decimal,
    pub transaction_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Tren {
    pub granularity: Butir,
    pub points: Vec<TitikTren>,
}

/// Rentang nyata saringan: sisi akhir yang terbuka berarti hari ini, sisi awal yang terbuka berarti hari pertama ada transaksi atau beban.
async fn rentang_nyata(pool: &PgPool, filter: &SalesFilter) -> AppResult<(NaiveDate, NaiveDate)> {
    let (dari, sampai) = filter.jendela();
    let sampai = sampai.unwrap_or_else(hari_ini);
    let dari = match dari {
        Some(d) => d,
        None => sqlx::query_scalar!(
            r#"
            SELECT LEAST(
                       (SELECT min((t.created_at AT TIME ZONE 'Asia/Jakarta')::date)
                        FROM transactions t WHERE t.status = 'completed'),
                       (SELECT min(e.expense_date) FROM expenses e)
                   ) AS awal
            "#
        )
        .fetch_one(pool)
        .await?
        .unwrap_or(sampai),
    };
    Ok((dari.min(sampai), sampai))
}

/// Tren mengikuti saringan (periode, metode bayar, jenis pembeli) termasuk batang kosong; deret dibuat `generate_series` di Postgres agar zona waktu tak ditiru di Rust.
pub async fn tren(pool: &PgPool, filter: &SalesFilter) -> AppResult<Tren> {
    let (dari, sampai) = rentang_nyata(pool, filter).await?;
    let butir = Butir::untuk(dari, sampai);
    let (awal, akhir) = butir.deret(dari, sampai);

    let rows = sqlx::query!(
        r#"
        WITH deret AS (
            SELECT generate_series($1::timestamp, $2::timestamp, ('1 ' || $3)::interval) AS awal
        ),
        jual AS (
            SELECT date_trunc($3, t.created_at AT TIME ZONE 'Asia/Jakarta') AS awal,
                   SUM(t.subtotal - t.discount_amount) AS omzet,
                   SUM(t.platform_fee_total)           AS potongan,
                   SUM(t.shipping_subsidy)             AS subsidi,
                   COUNT(*)                            AS jumlah
            FROM transactions t
            WHERE t.status = 'completed'
              AND t.created_at >= $4::date::timestamp AT TIME ZONE 'Asia/Jakarta'
              AND t.created_at < ($5::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta'
              AND ($6::text IS NULL OR t.payment_method = $6)
              AND ($7::text IS NULL OR t.type = $7)
            GROUP BY 1
        ),
        pokok AS (
            SELECT date_trunc($3, t.created_at AT TIME ZONE 'Asia/Jakarta') AS awal,
                   SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)) AS hpp
            FROM stock_adjustments sa
            JOIN transactions t ON t.id = sa.reference_id
            LEFT JOIN product_batches pb ON pb.id = sa.batch_id
            LEFT JOIN products p ON p.id = sa.product_id
            WHERE sa.reason = 'sale'
              AND sa.change_qty < 0
              AND sa.reference_type = 'transaction'
              AND t.status = 'completed'
              AND t.created_at >= $4::date::timestamp AT TIME ZONE 'Asia/Jakarta'
              AND t.created_at < ($5::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta'
              AND ($6::text IS NULL OR t.payment_method = $6)
              AND ($7::text IS NULL OR t.type = $7)
            GROUP BY 1
        ),
        beban AS (
            SELECT date_trunc($3, e.expense_date::timestamp) AS awal,
                   SUM(e.amount) AS jumlah
            FROM expenses e
            WHERE $3 <> 'hour'
              AND e.expense_date >= $4::date
              AND e.expense_date <= $5::date
            GROUP BY 1
        ),
        rugi AS (
            SELECT date_trunc($3, sa.created_at AT TIME ZONE 'Asia/Jakarta') AS awal,
                   SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)) AS jumlah
            FROM stock_adjustments sa
            LEFT JOIN product_batches pb ON pb.id = sa.batch_id
            LEFT JOIN products p ON p.id = sa.product_id
            WHERE sa.manual_reason IN ('rusak', 'kedaluwarsa', 'hilang')
              AND sa.created_at >= $4::date::timestamp AT TIME ZONE 'Asia/Jakarta'
              AND sa.created_at < ($5::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta'
            GROUP BY 1
        )
        SELECT d.awal                                          AS "start!",
               COALESCE(j.omzet, 0)                            AS "revenue!",
               COALESCE(k.hpp, 0)                              AS "cogs!",
               COALESCE(j.potongan, 0)                         AS "platform_fees!",
               COALESCE(b.jumlah, 0) + COALESCE(j.subsidi, 0)  AS "expenses!",
               COALESCE(r.jumlah, 0)                           AS "stock_loss!",
               COALESCE(j.jumlah, 0)                           AS "count!"
        FROM deret d
        LEFT JOIN jual j ON j.awal = d.awal
        LEFT JOIN pokok k ON k.awal = d.awal
        LEFT JOIN beban b ON b.awal = d.awal
        LEFT JOIN rugi r ON r.awal = d.awal
        ORDER BY d.awal
        "#,
        awal,
        akhir,
        butir.sql(),
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_all(pool)
    .await?;

    Ok(Tren {
        granularity: butir,
        points: rows
            .into_iter()
            .map(|r| TitikTren {
                start: r.start,
                profit: r.revenue - r.cogs - r.platform_fees - r.expenses - r.stock_loss,
                revenue: r.revenue,
                cogs: r.cogs,
                platform_fees: r.platform_fees,
                expenses: r.expenses,
                stock_loss: r.stock_loss,
                transaction_count: r.count,
            })
            .collect(),
    })
}

/// Satu kategori beban.
#[derive(Debug, Serialize)]
pub struct ExpenseSlice {
    pub category: String,
    pub amount: Decimal,
}

/// Beban per kategori; ongkir yang ditanggung toko ikut sebagai satu kategori sendiri dan mengikuti saringan transaksi, seperti `sales_summary`.
pub async fn expense_breakdown(
    pool: &PgPool,
    filter: &SalesFilter,
) -> AppResult<Vec<ExpenseSlice>> {
    let (dari, sampai) = filter.jendela();
    let rows = sqlx::query!(
        r#"
        SELECT x.category AS "category!", SUM(x.amount) AS "amount!"
        FROM (
            SELECT COALESCE(NULLIF(trim(e.category), ''), 'Lainnya') AS category,
                   e.amount
            FROM expenses e
            WHERE ($1::date IS NULL OR e.expense_date >= $1)
              AND ($2::date IS NULL OR e.expense_date <= $2)
            UNION ALL
            SELECT 'Ongkir ditanggung toko', t.shipping_subsidy
            FROM transactions t
            WHERE t.status = 'completed'
              AND t.shipping_subsidy > 0
              AND ($1::date IS NULL
                   OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($2::date IS NULL
                   OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($3::text IS NULL OR t.payment_method = $3)
              AND ($4::text IS NULL OR t.type = $4)
        ) x
        GROUP BY 1
        ORDER BY 2 DESC
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
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

/// Baris laba per produk; `revenue` di sini harga barang sebelum diskon karena diskon melekat pada transaksi dan membaginya ke barang berarti mengarang angka yang tak ada di struk.
#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_id: Option<Uuid>,
    /// Nama saat terjual, bukan nama sekarang, agar produk yang diganti namanya tetap dikenali di laporan lama.
    pub name: String,
    pub sku: Option<String>,
    pub qty: i64,
    /// Pack × ukuran; `null` bila produknya tak punya ukuran atau sudah dihapus.
    pub sold_kg: Option<Decimal>,
    pub revenue: Decimal,
    pub cogs: Decimal,
    /// `revenue - cogs`: laba kotor, sebelum potongan marketplace yang melekat pada transaksi, bukan pada barang.
    pub profit: Decimal,
    /// Baris keluaran stok tanpa harga beli, atau 1 bila barang ini tak punya keluaran stok sama sekali (barang lepas, produk terhapus); selama bukan nol `cogs` dan `profit` adalah batas atas.
    pub items_without_cost: i64,
}

/// Urutan daftar produk: terlaris untuk kartu Ringkasan, omzet/laba/marjin untuk tab Produk.
#[derive(Debug, Clone, Copy)]
pub enum UrutanProduk {
    Terjual,
    Omzet,
    Laba,
    Marjin,
}

impl UrutanProduk {
    pub fn parse(nilai: Option<&str>) -> Option<Self> {
        match nilai {
            None | Some("" | "qty") => Some(Self::Terjual),
            Some("revenue") => Some(Self::Omzet),
            Some("profit") => Some(Self::Laba),
            Some("margin") => Some(Self::Marjin),
            Some(_) => None,
        }
    }

    fn sql(self) -> &'static str {
        match self {
            Self::Terjual => "qty",
            Self::Omzet => "revenue",
            Self::Laba => "profit",
            Self::Marjin => "margin",
        }
    }
}

pub async fn top_products(
    pool: &PgPool,
    filter: &SalesFilter,
    urutan: UrutanProduk,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<TopProduct>> {
    let (dari, sampai) = filter.jendela();
    // Harga pokok dari keluaran stok per produk, dengan logika yang sama seperti `harga_pokok`.
    let rows = sqlx::query!(
        r#"
        WITH barang AS (
            SELECT ti.product_id,
                   max(ti.product_name_snapshot)  AS nama,
                   SUM(ti.qty)::bigint            AS qty,
                   SUM(ti.qty * p.variant_size)   AS kg,
                   SUM(ti.subtotal)               AS omzet
            FROM transaction_items ti
            JOIN transactions t ON t.id = ti.transaction_id
            LEFT JOIN products p ON p.id = ti.product_id
            WHERE t.status = 'completed'
              AND ($1::date IS NULL
                   OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($2::date IS NULL
                   OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($3::text IS NULL OR t.payment_method = $3)
              AND ($4::text IS NULL OR t.type = $4)
            GROUP BY ti.product_id, CASE WHEN ti.product_id IS NULL THEN ti.product_name_snapshot END
        ),
        pokok AS (
            SELECT sa.product_id,
                   SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)) AS hpp,
                   COUNT(*) FILTER (
                       WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL
                   ) AS tanpa_pokok
            FROM stock_adjustments sa
            JOIN transactions t ON t.id = sa.reference_id
            LEFT JOIN product_batches pb ON pb.id = sa.batch_id
            LEFT JOIN products p ON p.id = sa.product_id
            WHERE sa.reason = 'sale'
              AND sa.change_qty < 0
              AND sa.reference_type = 'transaction'
              AND t.status = 'completed'
              AND ($1::date IS NULL
                   OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($2::date IS NULL
                   OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($3::text IS NULL OR t.payment_method = $3)
              AND ($4::text IS NULL OR t.type = $4)
            GROUP BY sa.product_id
        )
        SELECT b.product_id                                AS product_id,
               b.nama                                      AS "name!",
               p.sku                                       AS sku,
               b.qty                                       AS "qty!",
               b.kg                                        AS sold_kg,
               b.omzet                                     AS "revenue!",
               COALESCE(k.hpp, 0)                          AS "cogs!",
               CASE WHEN k.product_id IS NULL THEN 1 ELSE k.tanpa_pokok END AS "tanpa_pokok!"
        FROM barang b
        LEFT JOIN pokok k ON k.product_id = b.product_id
        LEFT JOIN products p ON p.id = b.product_id
        ORDER BY
            CASE WHEN $5 = 'revenue' THEN b.omzet END DESC NULLS LAST,
            CASE WHEN $5 = 'profit' THEN b.omzet - COALESCE(k.hpp, 0) END DESC NULLS LAST,
            CASE WHEN $5 = 'margin' THEN (b.omzet - COALESCE(k.hpp, 0)) / NULLIF(b.omzet, 0) END DESC NULLS LAST,
            b.qty DESC, b.omzet DESC, b.product_id, b.nama
        LIMIT $6 OFFSET $7
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref(),
        urutan.sql(),
        limit,
        offset
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
            sold_kg: r.sold_kg,
            profit: r.revenue - r.cogs,
            revenue: r.revenue,
            cogs: r.cogs,
            items_without_cost: r.tanpa_pokok,
        })
        .collect())
}

/// Uang yang diterima toko per kanal dan metode bayar dalam satu butir waktu.
#[derive(Debug, Serialize)]
pub struct BarisUangMasuk {
    /// Awal butir pada zona toko, tanpa zona.
    pub start: NaiveDateTime,
    pub channel: String,
    /// Bermakna hanya untuk kanal toko; pesanan marketplace selalu dibayar lewat platform.
    pub payment_method: String,
    pub transaction_count: i64,
    pub revenue: Decimal,
    /// Ongkir yang ditagih ke pembeli.
    pub shipping: Decimal,
    /// `total_amount`: tagihan pembeli di toko (PPh tak memotongnya), uang cair di marketplace.
    pub received: Decimal,
}

#[derive(Debug, Serialize)]
pub struct UangMasuk {
    /// Per hari sampai dua bulan, selebihnya per bulan (butir jam tak dipakai karena uang dicocokkan per hari).
    pub granularity: Butir,
    pub rows: Vec<BarisUangMasuk>,
}

/// Uang masuk per butir, kanal, dan metode bayar; hanya periode yang menyaring karena yang dicocokkan adalah seluruh uang toko.
pub async fn uang_masuk(pool: &PgPool, filter: &SalesFilter) -> AppResult<UangMasuk> {
    let (dari, sampai) = rentang_nyata(pool, filter).await?;
    let butir = match Butir::untuk(dari, sampai) {
        Butir::Jam => Butir::Hari,
        lain => lain,
    };

    let rows = sqlx::query_as!(
        BarisUangMasuk,
        r#"
        SELECT date_trunc($3, t.created_at AT TIME ZONE 'Asia/Jakarta') AS "start!",
               t.sales_channel                      AS "channel!",
               t.payment_method                     AS "payment_method!",
               COUNT(*)                             AS "transaction_count!",
               SUM(t.subtotal - t.discount_amount)  AS "revenue!",
               SUM(t.shipping_charged)              AS "shipping!",
               SUM(t.total_amount)                  AS "received!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta'
          AND t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta'
        GROUP BY 1, 2, 3
        ORDER BY 1, 2, 3
        "#,
        dari,
        sampai,
        butir.sql()
    )
    .fetch_all(pool)
    .await?;

    Ok(UangMasuk {
        granularity: butir,
        rows,
    })
}

/// Jumlah baris produk terlaris tanpa batas, dasar jumlah halaman; harus mengelompokkan sama persis dengan `top_products`.
pub async fn top_products_count(pool: &PgPool, filter: &SalesFilter) -> AppResult<i64> {
    let (dari, sampai) = filter.jendela();
    let jumlah = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "jumlah!"
        FROM (
            SELECT 1
            FROM transaction_items ti
            JOIN transactions t ON t.id = ti.transaction_id
            WHERE t.status = 'completed'
              AND ($1::date IS NULL
                   OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($2::date IS NULL
                   OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
              AND ($3::text IS NULL OR t.payment_method = $3)
              AND ($4::text IS NULL OR t.type = $4)
            GROUP BY ti.product_id, CASE WHEN ti.product_id IS NULL THEN ti.product_name_snapshot END
        ) x
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref()
    )
    .fetch_one(pool)
    .await?;

    Ok(jumlah)
}

/// Satu baris daftar penjualan, bentuk ringkas untuk tabel dan ekspor tanpa seluruh item.
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
    offset: i64,
) -> AppResult<Vec<SaleRow>> {
    let (dari, sampai) = filter.jendela();
    let rows = sqlx::query!(
        r#"
        SELECT t.id                    AS "id!",
               t.created_at            AS "created_at!",
               c.name                  AS customer_name,
               t.type                  AS "transaction_type!",
               t.payment_method        AS "payment_method!",
               t.subtotal - t.discount_amount AS "revenue!",
               t.shipping_charged      AS "shipping!",
               t.total_amount          AS "total_amount!",
               (SELECT count(*) FROM transaction_items ti WHERE ti.transaction_id = t.id)
                                       AS "item_count!"
        FROM transactions t
        LEFT JOIN customers c ON c.id = t.customer_id
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($3::text IS NULL OR t.payment_method = $3)
          AND ($4::text IS NULL OR t.type = $4)
        ORDER BY t.created_at DESC, t.id
        LIMIT $5 OFFSET $6
        "#,
        dari,
        sampai,
        filter.payment_method.as_deref(),
        filter.transaction_type.as_deref(),
        limit,
        offset
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
    pub product_id: Option<Uuid>,
    /// Nama saat terjual, bukan nama sekarang -- lihat `TopProduct::name`.
    pub name: String,
    /// SKU produk saat ini (item transaksi tak menyimpan SKU), `null` bila produk sudah dihapus.
    pub sku: Option<String>,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

/// Detail lengkap satu transaksi untuk `/laporan/transaksi/{id}`; Owner saja karena memuat laba dan potongan Shopee.
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
    /// Diskon umum lama; nol untuk transaksi sejak migrasi 0023.
    pub discount_amount: Decimal,
    /// Ongkir sebenarnya; yang ditagih ke pembeli hanya bila `shipping_borne_by_store` false.
    pub shipping_cost: Decimal,
    pub shipping_borne_by_store: bool,
    pub total_amount: Decimal,
    pub amount_paid: Option<Decimal>,
    pub change_amount: Option<Decimal>,
    pub platform_commission_fee_percent: Decimal,
    pub platform_commission_fee: Decimal,
    pub platform_service_fee_percent: Decimal,
    pub platform_service_fee: Decimal,
    pub platform_dynamic_commission_fee_percent: Decimal,
    pub platform_dynamic_commission_fee: Decimal,
    pub platform_logistics_fee_percent: Decimal,
    pub platform_logistics_fee: Decimal,
    pub platform_withholding_tax: Decimal,
    pub platform_order_processing_fee: Decimal,
    /// Harga pokok baris yang keluar untuk transaksi ini, batas atas bila `items_without_cost > 0` (lihat `SalesSummary::cogs`).
    pub cogs: Decimal,
    pub items_without_cost: i64,
    /// `subtotal - discount_amount - cogs - platform_fee_total - ongkir ditanggung toko`; beban toko lain tak ikut karena milik periode.
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
               COALESCE(c.name, t.customer_name_snapshot) AS customer_name,
               c.phone                       AS customer_phone,
               c.email                       AS customer_email,
               u.name                        AS "cashier_name!",
               t.subtotal                    AS "subtotal!",
               t.discount_amount             AS "discount_amount!",
               t.shipping_cost               AS "shipping_cost!",
               t.shipping_borne_by_store     AS "shipping_borne_by_store!",
               t.shipping_subsidy            AS "shipping_subsidy!",
               t.total_amount                AS "total_amount!",
               t.amount_paid,
               t.change_amount,
               t.platform_commission_fee_percent AS "platform_commission_fee_percent!",
               t.platform_commission_fee         AS "platform_commission_fee!",
               t.platform_service_fee_percent    AS "platform_service_fee_percent!",
               t.platform_service_fee            AS "platform_service_fee!",
               t.platform_dynamic_commission_fee_percent AS "platform_dynamic_commission_fee_percent!",
               t.platform_dynamic_commission_fee AS "platform_dynamic_commission_fee!",
               t.platform_logistics_fee_percent  AS "platform_logistics_fee_percent!",
               t.platform_logistics_fee          AS "platform_logistics_fee!",
               t.platform_withholding_tax        AS "platform_withholding_tax!",
               t.platform_order_processing_fee   AS "platform_order_processing_fee!",
               t.platform_fee_total              AS "platform_fee_total!"
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
               ti.product_id             AS product_id,
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

    // Logikanya sama dengan query `pokok` di `sales_summary`, disaring ke satu transaksi lewat `reference_id`.
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
        shipping_borne_by_store: head.shipping_borne_by_store,
        total_amount: head.total_amount,
        amount_paid: head.amount_paid,
        change_amount: head.change_amount,
        platform_commission_fee_percent: head.platform_commission_fee_percent,
        platform_commission_fee: head.platform_commission_fee,
        platform_service_fee_percent: head.platform_service_fee_percent,
        platform_service_fee: head.platform_service_fee,
        platform_dynamic_commission_fee_percent: head.platform_dynamic_commission_fee_percent,
        platform_dynamic_commission_fee: head.platform_dynamic_commission_fee,
        platform_logistics_fee_percent: head.platform_logistics_fee_percent,
        platform_logistics_fee: head.platform_logistics_fee,
        platform_withholding_tax: head.platform_withholding_tax,
        platform_order_processing_fee: head.platform_order_processing_fee,
        cogs: pokok.cogs,
        items_without_cost: pokok.tanpa_pokok,
        net_profit: head.subtotal
            - head.discount_amount
            - pokok.cogs
            - head.platform_fee_total
            - head.shipping_subsidy,
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

/// Peringatan stok menipis, tipis dulu; hanya produk yang dijual (induk berevarian tak memegang stok sendiri).
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
    /// Batch bersisa tanpa harga beli di batch maupun produk dihitung nol, jadi selama bukan nol `value` adalah batas bawah.
    pub batches_without_cost: i64,
    /// Total berat stok dalam kg: sisa pack × `variant_size`; pack tanpa ukuran dihitung nol, jadi selama `packs_without_size` bukan nol ini batas bawah.
    pub total_kg: Decimal,
    /// Pack bersisa yang produknya belum punya ukuran, sehingga tak ikut `total_kg`.
    pub packs_without_size: i64,
}

/// Potret stok hari ini tanpa saringan periode; harga beli berurutan seperti `sales_summary` (batch, lalu `products.cost_price`); induk berevarian dilewati karena stok ada di varian.
pub async fn stock_value(pool: &PgPool) -> AppResult<StockValue> {
    let row = sqlx::query!(
        r#"
        SELECT COALESCE(
                   SUM(pb.remaining_qty * COALESCE(pb.purchase_price, p.cost_price, 0)),
                   0
               ) AS "value!",
               COUNT(*) FILTER (
                   WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL
               ) AS "tanpa_pokok!",
               COALESCE(SUM(pb.remaining_qty * p.variant_size), 0) AS "total_kg!",
               COALESCE(SUM(pb.remaining_qty) FILTER (WHERE p.variant_size IS NULL), 0)
                   AS "tanpa_ukuran!"
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
        total_kg: row.total_kg,
        packs_without_size: row.tanpa_ukuran,
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

/// Satu transaksi selesai pada laporan harian Google Sheets; `modal` berisi harga pokok per produk dalam urutan tetap supaya kolom Item 1..5 stabil antar penarikan.
#[derive(Debug, Serialize)]
pub struct BarisHarian {
    pub id: Uuid,
    pub kanal: String,
    pub pelanggan: Option<String>,
    /// Omzet barang `subtotal - discount_amount`, kolom Cash di template.
    pub bruto: Decimal,
    /// Uang yang benar-benar cair: `total_amount` (sudah dipotong semua potongan platform untuk Shopee dan Tokopedia).
    pub bersih: Decimal,
    pub pph: Decimal,
    pub komisi: Decimal,
    pub layanan: Decimal,
    /// Komisi dinamis Tokopedia, nol di kanal lain.
    pub komisi_dinamis: Decimal,
    /// Biaya layanan logistik Tokopedia, nol di kanal lain.
    pub logistik: Decimal,
    pub biaya_proses: Decimal,
    /// Ongkir yang ditanggung toko pada transaksi ini; beban harian yang tak ada di tabel `expenses`.
    pub subsidi_ongkir: Decimal,
    /// Ada barang tanpa harga pokok atau tak ada keluaran stok sama sekali, jadi modal dan profit baris ini batas atas.
    pub modal_tidak_lengkap: bool,
    pub modal: Vec<Decimal>,
}

/// Satu beban toko pada laporan harian Google Sheets.
#[derive(Debug, Serialize)]
pub struct PengeluaranHarian {
    pub kategori: Option<String>,
    pub keterangan: Option<String>,
    pub jumlah: Decimal,
}

pub async fn pengeluaran_harian(
    pool: &PgPool,
    tanggal: chrono::NaiveDate,
) -> AppResult<Vec<PengeluaranHarian>> {
    let baris = sqlx::query_as!(
        PengeluaranHarian,
        r#"
        SELECT e.category    AS kategori,
               e.description AS keterangan,
               e.amount      AS "jumlah!"
        FROM expenses e
        WHERE e.expense_date = $1
        ORDER BY e.created_at, e.id
        "#,
        tanggal
    )
    .fetch_all(pool)
    .await?;

    Ok(baris)
}

pub async fn laporan_harian(
    pool: &PgPool,
    tanggal: chrono::NaiveDate,
) -> AppResult<Vec<BarisHarian>> {
    let transaksi = sqlx::query!(
        r#"
        SELECT t.id                        AS "id!",
               t.sales_channel             AS "kanal!",
               COALESCE(c.name, t.customer_name_snapshot) AS pelanggan,
               t.subtotal - t.discount_amount AS "bruto!",
               t.total_amount              AS "bersih!",
               t.platform_withholding_tax  AS "pph!",
               t.platform_commission_fee   AS "komisi!",
               t.platform_service_fee      AS "layanan!",
               t.platform_dynamic_commission_fee AS "komisi_dinamis!",
               t.platform_logistics_fee    AS "logistik!",
               t.platform_order_processing_fee AS "biaya_proses!",
               t.shipping_subsidy          AS "subsidi_ongkir!"
        FROM transactions t
        LEFT JOIN customers c ON c.id = t.customer_id
        WHERE t.status = 'completed'
          AND (t.created_at AT TIME ZONE 'Asia/Jakarta')::date = $1
        ORDER BY t.created_at, t.id
        "#,
        tanggal
    )
    .fetch_all(pool)
    .await?;

    // Logika harga pokok sama dengan `sales_summary`, dikelompokkan per transaksi dan produk.
    let pokok = sqlx::query!(
        r#"
        SELECT sa.reference_id AS "transaksi_id!",
               SUM(ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price, 0)) AS "modal!",
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
          AND (t.created_at AT TIME ZONE 'Asia/Jakarta')::date = $1
        GROUP BY sa.reference_id, sa.product_id
        ORDER BY sa.reference_id, sa.product_id
        "#,
        tanggal
    )
    .fetch_all(pool)
    .await?;

    let mut modal: std::collections::HashMap<Uuid, (Vec<Decimal>, bool)> =
        std::collections::HashMap::new();
    for baris in pokok {
        let (daftar, tanpa) = modal.entry(baris.transaksi_id).or_default();
        daftar.push(baris.modal);
        *tanpa |= baris.tanpa_pokok > 0;
    }

    Ok(transaksi
        .into_iter()
        .map(|t| {
            let (modal, tanpa_pokok) = modal.remove(&t.id).unwrap_or_default();
            BarisHarian {
                modal_tidak_lengkap: tanpa_pokok || modal.is_empty(),
                modal,
                id: t.id,
                komisi: t.komisi,
                layanan: t.layanan,
                komisi_dinamis: t.komisi_dinamis,
                logistik: t.logistik,
                biaya_proses: t.biaya_proses,
                subsidi_ongkir: t.subsidi_ongkir,
                kanal: t.kanal,
                pelanggan: t.pelanggan,
                bruto: t.bruto,
                bersih: t.bersih,
                pph: t.pph,
            }
        })
        .collect())
}

// --- Stok: pembelian dan barang terbuang ---

/// Jumlah pembelian stok periode: batch yang masuk lewat "Catat batch" atau impor (punya ledger `restock`), bukan batch yang lahir dari koreksi naik.
#[derive(Debug, Serialize)]
pub struct RingkasanPembelian {
    pub batch_count: i64,
    pub qty: i64,
    /// Σ jumlah masuk × harga beli (cadangan `products.cost_price` seperti harga pokok).
    pub value: Decimal,
    /// Batch tanpa harga beli; selama bukan nol `value` batas bawah.
    pub batches_without_cost: i64,
}

#[derive(Debug, Serialize)]
pub struct BarisPembelian {
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub name: String,
    pub sku: Option<String>,
    pub batch_number: Option<String>,
    pub received_at: DateTime<Utc>,
    pub quantity: i32,
    pub remaining_qty: i32,
    pub purchase_price: Option<Decimal>,
    /// `quantity × purchase_price`; `null` bila harga beli kosong.
    pub total: Option<Decimal>,
}

/// Satu pengurangan stok manual: kerugian (`rusak`, `kedaluwarsa`, `hilang`), `salah_catat`, atau `null` untuk koreksi lama dan edit massal yang tak beralasan.
#[derive(Debug, Serialize)]
pub struct BarisPengurangan {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub product_id: Uuid,
    pub name: String,
    pub sku: Option<String>,
    pub qty: i32,
    pub reason: Option<String>,
    /// Jumlah × harga beli batch; `null` bila harga beli kosong.
    pub value: Option<Decimal>,
    pub adjusted_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LaporanStok {
    pub purchases: RingkasanPembelian,
    pub purchase_rows: Vec<BarisPembelian>,
    pub losses: Vec<KerugianStokSlice>,
    pub loss_total: Decimal,
    pub loss_without_cost: i64,
    pub reduction_rows: Vec<BarisPengurangan>,
    /// Seluruh pengurangan manual periode ini, dasar jumlah halaman `reduction_rows`.
    pub reduction_total: i64,
}

/// Jendela halaman dua daftar tab Stok.
pub struct HalamanStok {
    pub beli_limit: i64,
    pub beli_offset: i64,
    pub kurang_limit: i64,
    pub kurang_offset: i64,
}

pub async fn laporan_stok(
    pool: &PgPool,
    filter: &SalesFilter,
    h: &HalamanStok,
) -> AppResult<LaporanStok> {
    let (dari, sampai) = filter.jendela();

    let ringkasan = sqlx::query_as!(
        RingkasanPembelian,
        r#"
        SELECT COUNT(*)                                       AS "batch_count!",
               COALESCE(SUM(pb.quantity), 0)::bigint          AS "qty!",
               COALESCE(SUM(pb.quantity * COALESCE(pb.purchase_price, p.cost_price, 0)), 0) AS "value!",
               COUNT(*) FILTER (WHERE COALESCE(pb.purchase_price, p.cost_price) IS NULL) AS "batches_without_cost!"
        FROM product_batches pb
        JOIN products p ON p.id = pb.product_id
        WHERE EXISTS (SELECT 1 FROM stock_adjustments sa
                      WHERE sa.batch_id = pb.id AND sa.reason = 'restock' AND sa.change_qty > 0)
          AND ($1::date IS NULL
               OR pb.received_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR pb.received_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        "#,
        dari,
        sampai
    )
    .fetch_one(pool)
    .await?;

    let purchase_rows = sqlx::query_as!(
        BarisPembelian,
        r#"
        SELECT pb.id                 AS batch_id,
               p.id                  AS product_id,
               p.name                AS name,
               p.sku                 AS sku,
               pb.batch_number       AS batch_number,
               pb.received_at        AS received_at,
               pb.quantity           AS quantity,
               pb.remaining_qty      AS remaining_qty,
               COALESCE(pb.purchase_price, p.cost_price)                AS purchase_price,
               pb.quantity * COALESCE(pb.purchase_price, p.cost_price)  AS total
        FROM product_batches pb
        JOIN products p ON p.id = pb.product_id
        WHERE EXISTS (SELECT 1 FROM stock_adjustments sa
                      WHERE sa.batch_id = pb.id AND sa.reason = 'restock' AND sa.change_qty > 0)
          AND ($1::date IS NULL
               OR pb.received_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR pb.received_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        ORDER BY pb.received_at DESC, pb.id
        LIMIT $3 OFFSET $4
        "#,
        dari,
        sampai,
        h.beli_limit,
        h.beli_offset
    )
    .fetch_all(pool)
    .await?;

    let rugi = kerugian_stok(pool, filter).await?;

    let reduction_rows = sqlx::query_as!(
        BarisPengurangan,
        r#"
        SELECT sa.id                  AS id,
               sa.created_at          AS "created_at!",
               p.id                   AS product_id,
               p.name                 AS name,
               p.sku                  AS sku,
               ABS(sa.change_qty)     AS "qty!",
               sa.manual_reason       AS reason,
               ABS(sa.change_qty) * COALESCE(pb.purchase_price, p.cost_price) AS value,
               u.name                 AS "adjusted_by?"
        FROM stock_adjustments sa
        JOIN products p ON p.id = sa.product_id
        LEFT JOIN product_batches pb ON pb.id = sa.batch_id
        LEFT JOIN users u ON u.id = sa.adjusted_by_user_id
        WHERE sa.reason = 'manual_adjustment'
          AND sa.change_qty < 0
          AND ($1::date IS NULL
               OR sa.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR sa.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        ORDER BY sa.created_at DESC, sa.id
        LIMIT $3 OFFSET $4
        "#,
        dari,
        sampai,
        h.kurang_limit,
        h.kurang_offset
    )
    .fetch_all(pool)
    .await?;

    let reduction_total = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "jumlah!"
        FROM stock_adjustments sa
        WHERE sa.reason = 'manual_adjustment'
          AND sa.change_qty < 0
          AND ($1::date IS NULL
               OR sa.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR sa.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        "#,
        dari,
        sampai
    )
    .fetch_one(pool)
    .await?;

    Ok(LaporanStok {
        purchases: ringkasan,
        purchase_rows,
        losses: rugi.per_alasan,
        loss_total: rugi.total,
        loss_without_cost: rugi.tanpa_pokok,
        reduction_rows,
        reduction_total,
    })
}

// --- Void ---

/// Transaksi yang dibatalkan, disaring menurut waktu pembatalan karena yang diawasi adalah kapan dan oleh siapa void terjadi.
#[derive(Debug, Serialize)]
pub struct BarisVoid {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub voided_at: DateTime<Utc>,
    pub sales_channel: String,
    pub cashier: Option<String>,
    pub voided_by: Option<String>,
    pub void_reason: Option<String>,
    pub revenue: Decimal,
    pub total_amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct VoidPerOrang {
    /// Yang membatalkan; `null` bila akunnya sudah tak ada.
    pub name: Option<String>,
    pub count: i64,
    pub revenue: Decimal,
}

#[derive(Debug, Serialize)]
pub struct LaporanVoid {
    pub count: i64,
    pub revenue: Decimal,
    /// Transaksi selesai pada periode yang sama, pembanding persentase void.
    pub completed_count: i64,
    pub per_user: Vec<VoidPerOrang>,
    pub rows: Vec<BarisVoid>,
}

pub async fn laporan_void(
    pool: &PgPool,
    filter: &SalesFilter,
    limit: i64,
    offset: i64,
) -> AppResult<LaporanVoid> {
    let (dari, sampai) = filter.jendela();

    let per_user = sqlx::query_as!(
        VoidPerOrang,
        r#"
        SELECT u.name                                     AS "name?",
               COUNT(*)                                   AS "count!",
               SUM(t.subtotal - t.discount_amount)        AS "revenue!"
        FROM transactions t
        LEFT JOIN users u ON u.id = t.voided_by
        WHERE t.status = 'voided'
          AND ($1::date IS NULL
               OR t.voided_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.voided_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        GROUP BY t.voided_by, u.name
        ORDER BY 2 DESC, 1
        "#,
        dari,
        sampai
    )
    .fetch_all(pool)
    .await?;

    let rows = sqlx::query_as!(
        BarisVoid,
        r#"
        SELECT t.id                                 AS id,
               t.created_at                         AS "created_at!",
               t.voided_at                          AS "voided_at!",
               t.sales_channel                      AS "sales_channel!",
               k.name                               AS "cashier?",
               u.name                               AS "voided_by?",
               t.void_reason                        AS void_reason,
               t.subtotal - t.discount_amount       AS "revenue!",
               t.total_amount                       AS total_amount
        FROM transactions t
        LEFT JOIN users k ON k.id = t.cashier_user_id
        LEFT JOIN users u ON u.id = t.voided_by
        WHERE t.status = 'voided'
          AND ($1::date IS NULL
               OR t.voided_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.voided_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        ORDER BY t.voided_at DESC, t.id
        LIMIT $3 OFFSET $4
        "#,
        dari,
        sampai,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    let completed_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "jumlah!"
        FROM transactions t
        WHERE t.status = 'completed'
          AND ($1::date IS NULL
               OR t.created_at >= $1::date::timestamp AT TIME ZONE 'Asia/Jakarta')
          AND ($2::date IS NULL
               OR t.created_at < ($2::date + 1)::timestamp AT TIME ZONE 'Asia/Jakarta')
        "#,
        dari,
        sampai
    )
    .fetch_one(pool)
    .await?;

    Ok(LaporanVoid {
        count: per_user.iter().map(|o| o.count).sum(),
        revenue: per_user.iter().map(|o| o.revenue).sum(),
        completed_count,
        per_user,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tgl(t: i32, b: u32, h: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(t, b, h).unwrap()
    }

    #[test]
    fn awal_pekan_adalah_senin_dan_tahun_baru_adalah_1_januari() {
        assert_eq!(
            awal_periode(Periode::Pekan, tgl(2026, 11, 10)),
            tgl(2026, 11, 9)
        );
        assert_eq!(
            awal_periode(Periode::Pekan, tgl(2026, 11, 9)),
            tgl(2026, 11, 9)
        );
        assert_eq!(
            awal_periode(Periode::Bulan, tgl(2026, 11, 10)),
            tgl(2026, 11, 1)
        );
        assert_eq!(
            awal_periode(Periode::Tahun, tgl(2026, 11, 10)),
            tgl(2026, 1, 1)
        );
    }

    #[test]
    fn pembanding_tanggal_pilihan_sama_panjang_dan_menempel_di_depannya() {
        let filter = SalesFilter {
            rentang: Rentang::Tanggal {
                dari: Some(tgl(2026, 11, 8)),
                sampai: Some(tgl(2026, 11, 10)),
            },
            ..SalesFilter::default()
        };
        let (dari, sampai) = filter.sebelumnya().unwrap().jendela();
        assert_eq!(
            (dari, sampai),
            (Some(tgl(2026, 11, 5)), Some(tgl(2026, 11, 7)))
        );
    }

    #[test]
    fn butir_tren_mengikuti_panjang_rentang() {
        let hari = tgl(2026, 11, 14);
        assert_eq!(Butir::untuk(hari, hari), Butir::Jam);
        assert_eq!(Butir::untuk(tgl(2026, 11, 13), hari), Butir::Hari);
        assert_eq!(Butir::untuk(tgl(2026, 9, 14), hari), Butir::Hari);
        assert_eq!(Butir::untuk(tgl(2026, 9, 13), hari), Butir::Bulan);
    }

    #[test]
    fn deret_bulan_dibulatkan_ke_tanggal_satu_dan_deret_jam_sampai_pukul_23() {
        let (awal, akhir) = Butir::Bulan.deret(tgl(2026, 3, 15), tgl(2026, 9, 20));
        assert_eq!(awal, tgl(2026, 3, 1).and_hms_opt(0, 0, 0).unwrap());
        assert_eq!(akhir, tgl(2026, 9, 1).and_hms_opt(0, 0, 0).unwrap());

        let hari = tgl(2026, 11, 14);
        let (awal, akhir) = Butir::Jam.deret(hari, hari);
        assert_eq!(awal, hari.and_hms_opt(0, 0, 0).unwrap());
        assert_eq!(akhir, hari.and_hms_opt(23, 0, 0).unwrap());
    }

    #[test]
    fn urutan_produk_menolak_nilai_asing() {
        assert!(matches!(
            UrutanProduk::parse(None),
            Some(UrutanProduk::Terjual)
        ));
        assert!(matches!(
            UrutanProduk::parse(Some("margin")),
            Some(UrutanProduk::Marjin)
        ));
        assert!(UrutanProduk::parse(Some("acak")).is_none());
    }

    #[test]
    fn tanpa_pembanding_untuk_seluruh_waktu_dan_tanggal_setengah_terbuka() {
        assert!(SalesFilter::default().sebelumnya().is_none());
        let terbuka = SalesFilter {
            rentang: Rentang::Tanggal {
                dari: Some(tgl(2026, 11, 8)),
                sampai: None,
            },
            ..SalesFilter::default()
        };
        assert!(terbuka.sebelumnya().is_none());
    }
}
