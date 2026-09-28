//! Checkout POS.
//!
//! Dua hal yang paling mudah salah di sini, dan keduanya berakhir pada uang
//! atau stok yang keliru:
//!
//! 1. **Request yang terkirim dua kali.** Jaringan putus setelah server
//!    memproses tapi sebelum jawabannya sampai; kasir menekan "Bayar" lagi.
//!    Tanpa penjagaan, stok berkurang dua kali dan pembeli tertagih dua
//!    kali. Penjagaannya `Idempotency-Key` + sidik jari isi transaksi.
//! 2. **Barang yang sama dipindai dua kali.** Dua baris untuk satu produk
//!    harus digabung sebelum stok diperiksa -- ini ditangani `stock.rs`.

use super::repo::{self, NewTransaction, NewTransactionItem, Transaction as TxRow};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Transfer,
    Ewallet,
}

impl PaymentMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Transfer => "transfer",
            Self::Ewallet => "ewallet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionType {
    WalkIn,
    PreOrder,
}

impl TransactionType {
    fn as_str(self) -> &'static str {
        match self {
            Self::WalkIn => "walk_in",
            Self::PreOrder => "pre_order",
        }
    }
}

/// Kanal tempat penjualan ini terjadi, dan karena itu daftar harga mana yang
/// berlaku.
///
/// Shopee dan Tokopedia belum tersambung ke sistem ini, jadi pesanan dari
/// sana dicatat manual di kasir. Tanpa kanal, semuanya tercatat seharga toko
/// -- dan selisih harga marketplace muncul sebagai laba yang tidak ada.
///
/// `Tiktok` mencakup Tokopedia, mengikuti penamaan `products.price_tiktok`:
/// sejak TikTok mengakuisisi Tokopedia keduanya satu kanal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesChannel {
    Toko,
    Shopee,
    Tiktok,
}

impl SalesChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Toko => "toko",
            Self::Shopee => "shopee",
            Self::Tiktok => "tiktok",
        }
    }

    /// Harga yang berlaku di kanal ini. Harga kanal yang belum diatur
    /// (`None`) JATUH KE HARGA DASAR, bukan ke nol: `None` berarti "belum
    /// diatur", dan menjual seharga nol adalah kerugian langsung. Lihat
    /// migrasi 0005.
    fn harga(self, produk: &stock::LockedProduct) -> Decimal {
        match self {
            Self::Toko => produk.price,
            Self::Shopee => produk.price_shopee.unwrap_or(produk.price),
            Self::Tiktok => produk.price_tiktok.unwrap_or(produk.price),
        }
    }
}

/// Tarif Shopee. Nama fungsi mengikuti field ASLI di
/// `v2.payment.get_escrow_detail` milik Shopee (lihat migrasi 0018 dan
/// `docs/` hasil riset `congminh1254/shopee-sdk`), bukan istilah rakitan
/// sendiri:
///
/// - `commission_fee`: komisi dasar, berlaku semua pesanan.
///   `shopee_commission_persen_default` bisa DIGANTI kasir per transaksi
///   lewat `CheckoutInput::platform_commission_fee_percent`.
/// - `service_fee`: biaya program tambahan yang OPSIONAL (mis. Gratis
///   Ongkir Xtra, Star+). Defaultnya NOL -- kita tidak tahu toko ini ikut
///   program berbayar yang mana, jadi kasir yang mengisi kalau ternyata
///   berlaku. Juga bisa diedit lewat `CheckoutInput::platform_service_fee_percent`.
/// - `withholding_tax`: PPh final UMKM. Tetap 0,5%, tidak bisa diedit.
/// - `seller_order_processing_fee`: Rp1.250 tetap, SEKALI per pesanan.
///   Tidak bisa diedit.
///
/// Keempatnya disimpan per transaksi (migrasi 0017/0018), bukan dihitung
/// ulang di laporan dengan tarif global.
///
/// `Decimal::new` bukan `const fn` pada versi crate ini, jadi semuanya
/// fungsi kecil, bukan `const`.
fn shopee_commission_persen_default() -> Decimal {
    Decimal::new(1725, 4) // 17,25%
}
fn shopee_service_persen_default() -> Decimal {
    Decimal::ZERO
}
fn shopee_withholding_tax_persen() -> Decimal {
    Decimal::new(5, 3) // 0,5%
}
fn shopee_order_processing_fee() -> Decimal {
    Decimal::new(1250, 0)
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CheckoutItem {
    pub product_id: Uuid,
    pub qty: i32,
    /// Batch yang dipilih kasir. Tidak disebut berarti FEFO -- kedaluwarsa
    /// terdekat keluar lebih dulu.
    #[serde(default)]
    pub batch_id: Option<Uuid>,
}

#[derive(Debug)]
pub struct CheckoutInput {
    pub idempotency_key: String,
    pub transaction_type: TransactionType,
    pub customer_id: Option<Uuid>,
    pub payment_method: PaymentMethod,
    /// Daftar harga yang dipakai. Menentukan `unit_price` tiap baris struk.
    pub sales_channel: SalesChannel,
    pub amount_paid: Option<Decimal>,
    /// Ongkos kirim. `None` berarti nol -- bukan "tidak diketahui". Diabaikan
    /// sama sekali untuk `SalesChannel::Shopee` -- lihat `checkout`.
    pub shipping_cost: Option<Decimal>,
    /// Potongan harga atas seluruh belanja. `None` berarti nol. Tidak boleh
    /// melebihi subtotal -- lihat migrasi 0015.
    pub discount_amount: Option<Decimal>,
    /// Persentase `commission_fee` Shopee, sebagai pecahan (0,1725 = 17,25%).
    /// Hanya berlaku untuk `SalesChannel::Shopee`; diabaikan untuk kanal
    /// lain. `None` jatuh ke `shopee_commission_persen_default()`.
    pub platform_commission_fee_percent: Option<Decimal>,
    /// Persentase `service_fee` Shopee (program opsional). `None` jatuh ke
    /// `shopee_service_persen_default()` (nol).
    pub platform_service_fee_percent: Option<Decimal>,
    pub items: Vec<CheckoutItem>,
    pub cashier_user_id: Uuid,
}

/// Uang selalu dibulatkan ke 2 angka di belakang koma sebelum disimpan,
/// supaya angka di struk tidak pernah meleset beberapa sen dari yang
/// tercatat.
fn uang(nilai: Decimal) -> Decimal {
    nilai.round_dp(2)
}

/// Biaya platform Shopee dibulatkan ke RUPIAH PENUH (bukan `uang`, yang
/// menyisakan 2 desimal) -- harga barang di sistem ini selalu bulat, dan
/// menyimpan sisa sen dari perkalian persen (mis. Rp7.762,50) adalah
/// pecahan yang tidak pernah muncul di tempat lain.
///
/// Strateginya DIPAKSA `MidpointAwayFromZero`, bukan dibiarkan memakai
/// default `round_dp` (`MidpointNearestEven`, alias pembulatan ke genap
/// terdekat). Kasir kanal Shopee di kasir.astro menghitung ulang angka yang
/// sama ini lewat `Math.round` JavaScript SEBELUM transaksi disimpan, dan
/// `Math.round` selalu membulatkan ,50 ke ATAS. Kalau backend diam-diam
/// memakai aturan pembulatan yang berbeda, angka yang kasir lihat di layar
/// sebelum menekan "Selesaikan" bisa beda satu rupiah dari yang tercatat di
/// struk -- persis di titik seperti Rp45.000 × 17,25% = Rp7.762,50.
fn rupiah_bulat(nilai: Decimal) -> Decimal {
    nilai.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
}

/// Nominal uang sebagai bahan sidik jari, SELALU dua angka di belakang koma.
///
/// Nilai yang sama bisa tiba dengan skala berbeda: `70000` datang dari JSON
/// request, `70000.00` dibaca balik dari kolom `NUMERIC(14,2)`. `Decimal`
/// menganggap keduanya sama, tapi `to_string()` menghasilkan teks berbeda --
/// dan sidik jari bekerja di atas teks. Tanpa penyeragaman ini, pengiriman
/// ulang yang sah ditolak sebagai "isi berbeda", justru kegagalan yang
/// `Idempotency-Key` ada untuk mencegahnya.
fn sidik_uang(nilai: Option<Decimal>) -> String {
    nilai
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "null".into())
}

/// Sama seperti `sidik_uang`, tapi untuk persentase -- yang butuh presisi 4
/// angka di belakang koma (0,1725), bukan 2. Dipakai untuk
/// `platform_commission_fee_percent`/`platform_service_fee_percent`,
/// satu-satunya nominal non-uang yang ikut menentukan isi transaksi.
fn sidik_persen(nilai: Decimal) -> String {
    format!("{nilai:.4}")
}

/// Bahan sidik jari sebuah transaksi.
///
/// Dibungkus sebagai struct, bukan deretan argumen: isinya delapan hal yang
/// separuhnya bertipe sama, dan satu pasang yang tertukar di salah satu dari
/// dua tempat pemanggilan akan membuat pengiriman ulang yang sah ditolak --
/// kegagalan yang tidak akan terlihat sampai jaringan kasir sekali putus.
struct Bahan<'a> {
    transaction_type: TransactionType,
    sales_channel: SalesChannel,
    customer_id: Option<Uuid>,
    payment_method: PaymentMethod,
    amount_paid: Option<Decimal>,
    shipping_cost: Decimal,
    discount_amount: Decimal,
    /// Nol untuk kanal selain Shopee. Diikutkan karena persen yang berbeda
    /// berarti biaya yang tersimpan juga berbeda -- lihat migrasi 0018.
    platform_commission_fee_percent: Decimal,
    platform_service_fee_percent: Decimal,
    items: &'a [(Uuid, i32)],
}

/// Sidik jari isi transaksi, untuk mendeteksi `Idempotency-Key` yang dipakai
/// ulang dengan isi yang BERBEDA.
///
/// Bahannya sengaja hanya yang ikut tersimpan di database, supaya sidik jari
/// transaksi lama bisa dihitung ulang dari barisnya -- tidak perlu kolom
/// khusus. Item diurutkan lebih dulu agar urutan input yang berbeda tapi
/// isinya sama tetap dianggap sama.
fn sidik_jari(bahan: &Bahan<'_>) -> String {
    let mut urut: Vec<(Uuid, i32)> = bahan.items.to_vec();
    urut.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(bahan.transaction_type.as_str().as_bytes());
    hasher.update(b"|");
    // Kanal WAJIB ikut: ia menentukan daftar harga, jadi keranjang yang sama
    // di kanal berbeda adalah tagihan yang berbeda.
    hasher.update(bahan.sales_channel.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(
        bahan
            .customer_id
            .map(|c| c.to_string())
            .unwrap_or_else(|| "null".into())
            .as_bytes(),
    );
    hasher.update(b"|");
    hasher.update(bahan.payment_method.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(sidik_uang(bahan.amount_paid).as_bytes());
    hasher.update(b"|");
    // Ongkir WAJIB ikut. Tanpa ini, kasir yang sadar ongkirnya belum terisi
    // lalu mengirim ulang keranjang yang sama dengan ongkir baru akan
    // dianggap mengirim permintaan kembar: backend mengembalikan transaksi
    // lama, layar menampilkan struk yang kurang sebesar ongkirnya, dan tidak
    // ada galat di mana pun yang memberi tahu.
    hasher.update(sidik_uang(Some(bahan.shipping_cost)).as_bytes());
    hasher.update(b"|");
    // Diskon, dengan alasan yang sama persis seperti ongkir -- hanya arahnya
    // terbalik: yang terkirim ulang tanpa ini adalah struk yang KELEBIHAN
    // sebesar potongan yang baru saja disepakati di depan meja.
    hasher.update(sidik_uang(Some(bahan.discount_amount)).as_bytes());
    hasher.update(b"|");
    hasher.update(sidik_persen(bahan.platform_commission_fee_percent).as_bytes());
    hasher.update(b"|");
    hasher.update(sidik_persen(bahan.platform_service_fee_percent).as_bytes());
    for (product_id, qty) in urut {
        hasher.update(b"|");
        hasher.update(product_id.as_bytes());
        hasher.update(b":");
        hasher.update(qty.to_string().as_bytes());
    }

    hex::encode(hasher.finalize())
}

pub async fn checkout(pool: &PgPool, input: CheckoutInput) -> AppResult<TxRow> {
    if input.items.is_empty() {
        return Err(AppError::bad_request(
            "Transaksi harus punya minimal 1 item.",
        ));
    }
    if input.items.iter().any(|i| i.qty <= 0) {
        return Err(AppError::bad_request("Jumlah item harus lebih dari 0."));
    }

    // Transaksi Shopee dicatat sebagai uang yang sudah pasti diterima lewat
    // ShopeePay, bukan tunai yang dihitung kembaliannya di depan meja. Kalau
    // keduanya bercampur, "kembalian" jadi tidak berarti apa-apa karena
    // `total_amount`-nya sendiri sudah bersih dari potongan Shopee.
    if input.sales_channel == SalesChannel::Shopee && input.payment_method == PaymentMethod::Cash {
        return Err(AppError::bad_request(
            "Transaksi Shopee tidak bisa dicatat sebagai tunai -- uangnya diterima lewat ShopeePay.",
        ));
    }

    // Ongkir tidak berlaku sama sekali untuk Shopee -- logistiknya di luar
    // sistem ini, dan mencampurnya ke "Estimasi Total Penghasilan" (yang
    // sudah berarti "uang yang cair ke toko") memberi angka yang tidak
    // berarti apa-apa. Apa pun yang terkirim di kolom ini untuk kanal
    // Shopee diabaikan, bukan ditolak -- klien lama yang belum tahu kolom
    // ini sudah tidak berlaku untuk Shopee tidak perlu error.
    //
    // Dinormalkan sekali di sini lalu dipakai untuk sidik jari MAUPUN total,
    // supaya angka yang di-hash persis angka yang tersimpan.
    let ongkir = if input.sales_channel == SalesChannel::Shopee {
        Decimal::ZERO
    } else {
        let nilai = uang(input.shipping_cost.unwrap_or(Decimal::ZERO));
        if nilai.is_sign_negative() {
            return Err(AppError::bad_request("Ongkos kirim tidak boleh negatif."));
        }
        nilai
    };

    // Dinormalkan bersama ongkir, dan dengan alasan yang sama: angka yang
    // di-hash harus persis angka yang tersimpan. Batas atasnya -- tidak
    // melebihi subtotal -- baru bisa diperiksa setelah harga barang dibaca
    // dari database, jadi pemeriksaannya ada di bawah.
    let diskon = uang(input.discount_amount.unwrap_or(Decimal::ZERO));
    if diskon.is_sign_negative() {
        return Err(AppError::bad_request("Diskon tidak boleh negatif."));
    }

    // Diselesaikan di sini, sebelum sidik jari -- persennya bagian dari apa
    // yang diminta kasir, bukan sesuatu yang bergantung pada harga barang.
    // Nol untuk kanal selain Shopee: mengirimkan persen untuk kanal lain
    // tidak berarti apa-apa dan tidak boleh ikut tersimpan (lihat migrasi
    // 0018, `transactions_platform_fee_channel_check`).
    let jepit_persen = |persen: Decimal, label: &str| -> AppResult<Decimal> {
        if persen.is_sign_negative() || persen > Decimal::ONE {
            return Err(AppError::bad_request(format!(
                "Persentase {label} harus di antara 0% dan 100%."
            )));
        }
        Ok(persen)
    };

    let (commission_persen, service_persen) = match input.sales_channel {
        SalesChannel::Shopee => (
            jepit_persen(
                input
                    .platform_commission_fee_percent
                    .unwrap_or_else(shopee_commission_persen_default),
                "biaya komisi",
            )?,
            jepit_persen(
                input
                    .platform_service_fee_percent
                    .unwrap_or_else(shopee_service_persen_default),
                "biaya layanan",
            )?,
        ),
        _ => (Decimal::ZERO, Decimal::ZERO),
    };

    // Digabung DULU, baru disidikjari: yang tersimpan di database juga versi
    // gabungannya, jadi sidik jari request dan sidik jari transaksi lama
    // dihitung dari bahan yang bentuknya sama persis.
    let lines: Vec<StockLine> = stock::gabungkan_baris_kembar(
        &input
            .items
            .iter()
            .map(|i| StockLine {
                product_id: i.product_id,
                qty: i.qty,
                batch_id: i.batch_id,
            })
            .collect::<Vec<_>>(),
    );

    // Ditulis persis seperti yang nanti benar-benar tersimpan: non-tunai
    // selalu berakhir null (tidak ada kembalian), dan nominal tunai
    // dibulatkan lebih dulu. Kalau tidak begitu, request ulang yang sah bisa
    // salah dianggap "isinya berbeda".
    let amount_paid_tersimpan = match input.payment_method {
        PaymentMethod::Cash => input.amount_paid.map(uang),
        _ => None,
    };

    // Harga, baris struk, dan sidik jari bekerja PER PRODUK, bukan per
    // batch. Pembeli membeli barang; dari kiriman mana barang itu diambil
    // tidak mengubah apa yang dia bayar. Ini juga yang membuat pengiriman
    // ulang yang sah tetap dikenali walaupun pilihan batch-nya berbeda --
    // yang dijaga `Idempotency-Key` adalah "jangan menagih dua kali", dan
    // itu soal isi belanja, bukan soal rak.
    let per_produk = stock::total_per_produk(&lines);
    let sidik = sidik_jari(&Bahan {
        transaction_type: input.transaction_type,
        sales_channel: input.sales_channel,
        customer_id: input.customer_id,
        payment_method: input.payment_method,
        amount_paid: amount_paid_tersimpan,
        shipping_cost: ongkir,
        discount_amount: diskon,
        platform_commission_fee_percent: commission_persen,
        platform_service_fee_percent: service_persen,
        items: &per_produk,
    });

    // Request ulang yang sah: kembalikan transaksi yang sudah ada, tanpa
    // menyentuh stok lagi.
    if let Some(existing) = repo::find_by_idempotency_key(pool, &input.idempotency_key).await? {
        let items_lama = repo::item_bahan_sidik_jari(pool, existing.id).await?;
        let sidik_lama = sidik_jari(&Bahan {
            transaction_type: parse_type(&existing.transaction_type)?,
            sales_channel: parse_channel(&existing.sales_channel)?,
            customer_id: existing.customer_id,
            payment_method: parse_payment(&existing.payment_method)?,
            amount_paid: existing.amount_paid,
            shipping_cost: existing.shipping_cost,
            discount_amount: existing.discount_amount,
            platform_commission_fee_percent: existing.platform_commission_fee_percent,
            platform_service_fee_percent: existing.platform_service_fee_percent,
            items: &items_lama,
        });

        if sidik_lama == sidik {
            return Ok(existing);
        }

        return Err(AppError::conflict(
            "Idempotency-Key ini sudah dipakai untuk transaksi dengan isi berbeda.",
        ));
    }

    let mut tx = pool.begin().await?;

    let ids: Vec<Uuid> = lines.iter().map(|l| l.product_id).collect();
    let terkunci = stock::kunci_produk(&mut tx, &ids).await?;

    let mut items = Vec::with_capacity(per_produk.len());
    let mut subtotal = Decimal::ZERO;

    for (product_id, qty) in per_produk.iter().copied() {
        let product = terkunci
            .get(&product_id)
            .ok_or_else(|| AppError::not_found("Produk tidak ditemukan."))?;

        // Harga diambil dari database, bukan dari request -- kalau client
        // yang menentukan harga, siapa pun yang bisa memanggil API ini bisa
        // membeli apa saja seharga nol. Yang datang dari request hanyalah
        // KANAL-nya: daftar harga mana yang berlaku, bukan angkanya.
        let unit_price = uang(input.sales_channel.harga(product));
        let baris_subtotal = uang(unit_price * Decimal::from(qty));
        subtotal += baris_subtotal;

        items.push(NewTransactionItem {
            product_id: product.id,
            product_name_snapshot: product.name.clone(),
            qty,
            unit_price,
            subtotal: baris_subtotal,
        });
    }

    let subtotal = uang(subtotal);

    // Diskon tidak boleh melebihi harga barangnya. Batas ini juga ada sebagai
    // CHECK di database (migrasi 0015); di sini supaya kasir mendapat kalimat
    // yang bisa dibaca, bukan galat constraint.
    if diskon > subtotal {
        return Err(AppError::bad_request(
            "Diskon tidak boleh melebihi subtotal belanja.",
        ));
    }

    // Potongan Shopee: commission_fee (bisa diedit) + service_fee (bisa
    // diedit, default nol) + withholding_tax 0,5% (tetap), ketiganya dari
    // omzet SETELAH diskon (ongkir sudah nol untuk Shopee, jadi tidak perlu
    // dikecualikan lagi di sini). Plus seller_order_processing_fee Rp1.250
    // tetap, sekali per transaksi. Nol untuk kanal selain Shopee.
    //
    // `rupiah_bulat`, bukan `uang`: lihat komentarnya -- membulatkan ke
    // rupiah penuh dengan strategi yang SAMA dengan `Math.round` di
    // kasir.astro, supaya angka yang kasir lihat sebelum membayar sama
    // persis dengan yang tersimpan di struk.
    let basis_fee = subtotal - diskon;
    let (biaya_komisi, biaya_layanan, pph, biaya_proses) =
        if input.sales_channel == SalesChannel::Shopee {
            (
                rupiah_bulat(basis_fee * commission_persen),
                rupiah_bulat(basis_fee * service_persen),
                rupiah_bulat(basis_fee * shopee_withholding_tax_persen()),
                shopee_order_processing_fee(),
            )
        } else {
            (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
        };

    // `subtotal` tetap harga barang saja; diskon dan ongkir punya kolomnya
    // sendiri dan hanya bertemu di `total_amount`. Itulah yang membuat baris
    // struk tetap bisa dijumlahkan menjadi subtotal, dan sekaligus membuat
    // pemeriksaan uang tunai serta kembalian di bawah otomatis benar tanpa
    // disentuh -- keduanya sudah memakai `total_amount`.
    //
    // Untuk Shopee, `total_amount` di sini BUKAN "yang dibayar pembeli" --
    // pembeli sudah membayar penuh lewat Shopee. Ia adalah uang yang
    // sungguh cair ke toko setelah potongan Shopee: itulah angka yang benar
    // untuk dicatat, karena layar kasir ini memang cuma untuk mencatat apa
    // yang sudah terjadi, bukan menagih pembayaran baru.
    let total_amount =
        uang(subtotal - diskon + ongkir - biaya_komisi - biaya_layanan - pph - biaya_proses);

    if input.payment_method == PaymentMethod::Cash {
        let dibayar = amount_paid_tersimpan
            .ok_or_else(|| AppError::bad_request("Nominal pembayaran tunai wajib diisi."))?;
        if dibayar < total_amount {
            return Err(AppError::bad_request(
                "Nominal pembayaran kurang dari total belanja.",
            ));
        }
    }

    let change_amount = amount_paid_tersimpan.map(|dibayar| uang(dibayar - total_amount));

    let transaction_id = repo::insert_transaction(
        &mut tx,
        &NewTransaction {
            idempotency_key: input.idempotency_key.clone(),
            transaction_type: input.transaction_type.as_str().to_string(),
            customer_id: input.customer_id,
            cashier_user_id: input.cashier_user_id,
            payment_method: input.payment_method.as_str().to_string(),
            sales_channel: input.sales_channel.as_str().to_string(),
            subtotal,
            discount_amount: diskon,
            shipping_cost: ongkir,
            total_amount,
            platform_commission_fee_percent: commission_persen,
            platform_commission_fee: biaya_komisi,
            platform_service_fee_percent: service_persen,
            platform_service_fee: biaya_layanan,
            platform_withholding_tax: pph,
            platform_order_processing_fee: biaya_proses,
            amount_paid: amount_paid_tersimpan,
            change_amount,
        },
        &items,
    )
    .await?;

    // Baris produk sudah terkunci di transaksi ini, jadi penguncian di dalam
    // `kurangi` tidak menunggu siapa pun. Pemeriksaan stoknya tetap berjalan
    // dan tetap menjadi satu-satunya tempat stok berubah.
    stock::kurangi(
        &mut tx,
        &lines,
        StockReason::Sale,
        transaction_id,
        Some(input.cashier_user_id),
    )
    .await?;

    tx.commit().await?;

    repo::find_by_id(pool, transaction_id)
        .await?
        .ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))
}

fn parse_type(raw: &str) -> AppResult<TransactionType> {
    match raw {
        "walk_in" => Ok(TransactionType::WalkIn),
        "pre_order" => Ok(TransactionType::PreOrder),
        _ => Err(AppError::conflict("Tipe transaksi lama tidak dikenal.")),
    }
}

fn parse_channel(raw: &str) -> AppResult<SalesChannel> {
    match raw {
        "toko" => Ok(SalesChannel::Toko),
        "shopee" => Ok(SalesChannel::Shopee),
        "tiktok" => Ok(SalesChannel::Tiktok),
        _ => Err(AppError::conflict("Kanal penjualan lama tidak dikenal.")),
    }
}

fn parse_payment(raw: &str) -> AppResult<PaymentMethod> {
    match raw {
        "cash" => Ok(PaymentMethod::Cash),
        "transfer" => Ok(PaymentMethod::Transfer),
        "ewallet" => Ok(PaymentMethod::Ewallet),
        _ => Err(AppError::conflict("Metode pembayaran lama tidak dikenal.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn produk(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// Bahan sidik jari dengan isi paling biasa. Tiap pengujian mengubah
    /// SATU hal darinya, supaya yang diuji benar-benar hal itu.
    fn bahan(items: &[(Uuid, i32)]) -> Bahan<'_> {
        Bahan {
            transaction_type: TransactionType::WalkIn,
            sales_channel: SalesChannel::Toko,
            customer_id: None,
            payment_method: PaymentMethod::Cash,
            amount_paid: Some(Decimal::new(10000, 0)),
            shipping_cost: Decimal::ZERO,
            discount_amount: Decimal::ZERO,
            platform_commission_fee_percent: Decimal::ZERO,
            platform_service_fee_percent: Decimal::ZERO,
            items,
        }
    }

    #[test]
    fn urutan_item_tidak_mengubah_sidik_jari() {
        let maju = [(produk(1), 2), (produk(2), 1)];
        let mundur = [(produk(2), 1), (produk(1), 2)];

        assert_eq!(sidik_jari(&bahan(&maju)), sidik_jari(&bahan(&mundur)));
    }

    #[test]
    fn isi_yang_berbeda_menghasilkan_sidik_jari_berbeda() {
        let items = [(produk(1), 2)];
        let lebih_banyak = [(produk(1), 3)];
        let dasar = sidik_jari(&bahan(&items));

        assert_ne!(dasar, sidik_jari(&bahan(&lebih_banyak)));

        let bayar_beda = Bahan {
            amount_paid: Some(Decimal::new(20000, 0)),
            ..bahan(&items)
        };
        assert_ne!(dasar, sidik_jari(&bayar_beda));

        let metode_beda = Bahan {
            payment_method: PaymentMethod::Transfer,
            ..bahan(&items)
        };
        assert_ne!(dasar, sidik_jari(&metode_beda));
    }

    #[test]
    fn pembulatan_uang_konsisten_dua_desimal() {
        assert_eq!(uang(Decimal::new(1005, 3)), Decimal::new(100, 2));
        assert_eq!(uang(Decimal::new(70000, 0)), Decimal::new(70000, 0));
    }

    #[test]
    fn biaya_platform_dibulatkan_ke_rupiah_penuh_bukan_genap_terdekat() {
        // Rp45.000 x 17,25% = Rp7.762,50 PERSIS di tengah dua rupiah. Default
        // `round_dp` rust_decimal membulatkan ke GENAP terdekat (7762,50 ->
        // 7762, karena 7762 genap) -- kalau `rupiah_bulat` diam-diam kembali
        // memakai itu, kasir yang melihat Rp7.763 di layar (dihitung
        // `Math.round` JavaScript, yang selalu membulatkan ,50 ke atas) akan
        // menerima struk yang mencatat Rp7.762. Uji ini mengunci arahnya.
        let basis = Decimal::new(45000, 0);
        let persen = Decimal::new(1725, 4); // 17,25%
        assert_eq!(rupiah_bulat(basis * persen), Decimal::new(7763, 0));

        // Kasus non-tengah tetap harus membulatkan seperti biasa.
        assert_eq!(rupiah_bulat(Decimal::new(72249, 2)), Decimal::new(722, 0));
    }

    #[test]
    fn skala_desimal_tidak_mengubah_sidik_jari() {
        // Nominal yang sama bisa sampai ke fungsi ini dengan skala berbeda:
        // `70000` datang dari JSON request (skala 0), `70000.00` dibaca balik
        // dari kolom NUMERIC(14,2) (skala 2). Keduanya rupiah yang sama.
        //
        // Kalau sidik jarinya berbeda, pengiriman ulang yang SAH -- jaringan
        // putus lalu kasir menekan Bayar lagi -- dijawab "isi berbeda", justru
        // kegagalan yang Idempotency-Key ada untuk mencegahnya.
        let items = [(produk(1), 1)];
        let dari_request = Bahan {
            amount_paid: Some(Decimal::new(70000, 0)),
            ..bahan(&items)
        };
        let dari_database = Bahan {
            amount_paid: Some(Decimal::new(7000000, 2)),
            ..bahan(&items)
        };

        assert_eq!(sidik_jari(&dari_request), sidik_jari(&dari_database));
    }

    #[test]
    fn ongkir_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Keranjang yang sama dengan ongkir berbeda adalah transaksi yang
        // BERBEDA. Kalau sidik jarinya sama, mengirim ulang setelah ongkir
        // diperbaiki akan mengembalikan transaksi lama yang nilainya kurang --
        // tanpa galat apa pun yang memberi tahu.
        let items = [(produk(1), 1)];
        let dengan_ongkir = Bahan {
            shipping_cost: Decimal::new(20000, 0),
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&dengan_ongkir));
    }

    #[test]
    fn diskon_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Sama seperti ongkir, arah sebaliknya: tanpa diskon di sidik jari,
        // kasir yang mengirim ulang keranjang setelah menyepakati potongan
        // akan menagih pembeli penuh -- dan struk yang tercetak adalah struk
        // lama yang tidak pernah kena potongan.
        let items = [(produk(1), 1)];
        let dengan_diskon = Bahan {
            discount_amount: Decimal::new(5000, 0),
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&dengan_diskon));
    }

    #[test]
    fn persen_komisi_shopee_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Kasir bisa mengedit persen commission_fee Shopee per transaksi.
        // Tanpa ini di sidik jari, mengoreksi persen yang salah lalu mengirim
        // ulang keranjang yang sama akan dijawab dengan transaksi lama yang
        // biaya platformnya tersimpan salah.
        let items = [(produk(1), 1)];
        let persen_beda = Bahan {
            platform_commission_fee_percent: Decimal::new(20, 2), // 20%
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&persen_beda));
    }

    #[test]
    fn persen_layanan_shopee_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Sama seperti komisi, tapi untuk service_fee -- program opsional
        // yang persennya juga bisa diedit kasir.
        let items = [(produk(1), 1)];
        let persen_beda = Bahan {
            platform_service_fee_percent: Decimal::new(5, 2), // 5%
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&persen_beda));
    }

    #[test]
    fn kanal_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Keranjang yang sama di kanal berbeda memakai daftar harga berbeda,
        // jadi tagihannya berbeda. Tanpa kanal di sidik jari, membetulkan
        // kanal lalu mengirim ulang akan dijawab dengan transaksi lama yang
        // harganya salah.
        let items = [(produk(1), 1)];
        let shopee = Bahan {
            sales_channel: SalesChannel::Shopee,
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&shopee));
    }

    #[test]
    fn harga_kanal_jatuh_ke_harga_dasar_saat_belum_diatur() {
        // `None` berarti "belum diatur", bukan "gratis". Menjual seharga nol
        // adalah kerugian langsung, dan produk yang harga marketplace-nya
        // belum sempat diisi adalah keadaan yang biasa, bukan luar biasa.
        let produk = stock::LockedProduct {
            id: produk(1),
            name: "Ceker Bersih".into(),
            price: Decimal::new(25000, 0),
            price_shopee: Some(Decimal::new(28000, 0)),
            price_tiktok: None,
            stock_qty: 10,
        };

        assert_eq!(SalesChannel::Toko.harga(&produk), Decimal::new(25000, 0));
        assert_eq!(SalesChannel::Shopee.harga(&produk), Decimal::new(28000, 0));
        assert_eq!(SalesChannel::Tiktok.harga(&produk), Decimal::new(25000, 0));
    }
}
