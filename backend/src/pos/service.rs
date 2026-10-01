//! Checkout POS; dua risiko utama: request terkirim dua kali (dijaga `Idempotency-Key` + sidik jari isi) dan barang sama dipindai dua kali (digabung sebelum cek stok di `stock.rs`).

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

/// Kanal penjualan menentukan daftar harga (Shopee/Tokopedia dicatat manual di kasir, tanpa kanal selisih harga jadi laba palsu); `Tiktok` mencakup Tokopedia mengikuti `products.price_tiktok`.
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

    /// Harga kanal yang `None` jatuh ke harga dasar, bukan nol (menjual nol adalah kerugian), lihat migrasi 0005.
    fn harga(self, produk: &stock::LockedProduct) -> Decimal {
        match self {
            Self::Toko => produk.price,
            Self::Shopee => produk.price_shopee.unwrap_or(produk.price),
            Self::Tiktok => produk.price_tiktok.unwrap_or(produk.price),
        }
    }
}

/// Tarif Shopee mengikuti `get_escrow_detail` (migrasi 0018): komisi bisa diganti kasir, service fee default nol, PPh 0,5% dan Rp1.250 tetap; fungsi karena `Decimal::new` bukan `const fn`.
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
    /// Batch yang dipilih kasir; tak disebut berarti FEFO (kedaluwarsa terdekat dulu).
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
    /// Ongkos kirim; `None` berarti nol, diabaikan untuk `SalesChannel::Shopee` (lihat `checkout`).
    pub shipping_cost: Option<Decimal>,
    /// Potongan harga seluruh belanja; `None` berarti nol, tak boleh melebihi subtotal (migrasi 0015).
    pub discount_amount: Option<Decimal>,
    /// Persentase `commission_fee` Shopee sebagai pecahan, hanya `SalesChannel::Shopee`; `None` jatuh ke `shopee_commission_persen_default()`.
    pub platform_commission_fee_percent: Option<Decimal>,
    /// Persentase `service_fee` Shopee (opsional); `None` jatuh ke `shopee_service_persen_default()` (nol).
    pub platform_service_fee_percent: Option<Decimal>,
    pub items: Vec<CheckoutItem>,
    pub cashier_user_id: Uuid,
}

/// Uang dibulatkan 2 desimal sebelum disimpan agar angka struk tak meleset sen dari yang tercatat.
fn uang(nilai: Decimal) -> Decimal {
    nilai.round_dp(2)
}

/// Biaya Shopee dibulatkan ke rupiah penuh dengan `MidpointAwayFromZero` agar sama dengan `Math.round` di kasir.astro (,50 ke atas), bukan default `round_dp` ke genap.
fn rupiah_bulat(nilai: Decimal) -> Decimal {
    nilai.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
}

/// Nominal uang untuk sidik jari selalu dua desimal: `70000` (JSON) dan `70000.00` (NUMERIC) sama untuk `Decimal` tapi beda teks, dan tanpa penyeragaman kirim ulang sah ditolak sebagai "isi berbeda".
fn sidik_uang(nilai: Option<Decimal>) -> String {
    nilai
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "null".into())
}

/// Seperti `sidik_uang` tapi persentase dengan 4 desimal (0,1725), untuk `platform_commission_fee_percent`/`platform_service_fee_percent`.
fn sidik_persen(nilai: Decimal) -> String {
    format!("{nilai:.4}")
}

/// Bahan sidik jari berupa struct, bukan argumen, karena delapan isinya setengah bertipe sama dan pertukaran di satu pemanggil akan menolak kirim ulang sah.
struct Bahan<'a> {
    transaction_type: TransactionType,
    sales_channel: SalesChannel,
    customer_id: Option<Uuid>,
    payment_method: PaymentMethod,
    amount_paid: Option<Decimal>,
    shipping_cost: Decimal,
    discount_amount: Decimal,
    /// Nol selain Shopee; diikutkan karena persen berbeda berarti biaya tersimpan berbeda (migrasi 0018).
    platform_commission_fee_percent: Decimal,
    platform_service_fee_percent: Decimal,
    items: &'a [(Uuid, i32)],
}

/// Sidik jari isi transaksi untuk mendeteksi `Idempotency-Key` dipakai ulang dengan isi berbeda; bahannya hanya yang tersimpan di DB agar bisa dihitung ulang dari baris lama, dan item diurutkan.
fn sidik_jari(bahan: &Bahan<'_>) -> String {
    let mut urut: Vec<(Uuid, i32)> = bahan.items.to_vec();
    urut.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(bahan.transaction_type.as_str().as_bytes());
    hasher.update(b"|");
    // Kanal wajib ikut karena menentukan daftar harga: keranjang sama di kanal berbeda adalah tagihan berbeda.
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
    // Ongkir wajib ikut; tanpanya kirim ulang dengan ongkir baru dianggap kembar dan layar menampilkan struk kurang ongkir tanpa galat.
    hasher.update(sidik_uang(Some(bahan.shipping_cost)).as_bytes());
    hasher.update(b"|");
    // Diskon dengan alasan sama seperti ongkir, arahnya terbalik: tanpa ini struk yang tertampil kelebihan sebesar potongan yang baru disepakati.
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

    // Transaksi Shopee dicatat sebagai uang yang pasti diterima lewat ShopeePay, bukan tunai berkembalian, karena `total_amount` sudah bersih dari potongan Shopee.
    if input.sales_channel == SalesChannel::Shopee && input.payment_method == PaymentMethod::Cash {
        return Err(AppError::bad_request(
            "Transaksi Shopee tidak bisa dicatat sebagai tunai -- uangnya diterima lewat ShopeePay.",
        ));
    }

    // Ongkir tak berlaku untuk Shopee: nilai terkirim diabaikan (bukan ditolak) dan dinormalkan sekali agar yang di-hash = yang tersimpan.
    let ongkir = if input.sales_channel == SalesChannel::Shopee {
        Decimal::ZERO
    } else {
        let nilai = uang(input.shipping_cost.unwrap_or(Decimal::ZERO));
        if nilai.is_sign_negative() {
            return Err(AppError::bad_request("Ongkos kirim tidak boleh negatif."));
        }
        nilai
    };

    // Dinormalkan bersama ongkir (yang di-hash harus yang tersimpan); batas atas subtotal baru bisa dicek setelah harga dibaca dari DB, di bawah.
    let diskon = uang(input.discount_amount.unwrap_or(Decimal::ZERO));
    if diskon.is_sign_negative() {
        return Err(AppError::bad_request("Diskon tidak boleh negatif."));
    }

    // Diselesaikan sebelum sidik jari karena persen bagian dari permintaan kasir; nol selain Shopee dan tak boleh ikut tersimpan (migrasi 0018, `transactions_platform_fee_channel_check`).
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

    // Digabung dulu baru disidikjari, karena yang tersimpan juga versi gabungan sehingga sidik jari request dan transaksi lama berbahan sama.
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

    // Ditulis persis seperti yang akan tersimpan (non-tunai selalu null, nominal tunai dibulatkan), kalau tidak request ulang sah dianggap "isi berbeda".
    let amount_paid_tersimpan = match input.payment_method {
        PaymentMethod::Cash => input.amount_paid.map(uang),
        _ => None,
    };

    // Harga, baris struk, dan sidik jari bekerja per produk, bukan per batch (pembeli membeli barang, bukan kiriman), sehingga kirim ulang sah tetap dikenali walau batch berbeda.
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

    // Request ulang sah: kembalikan transaksi yang ada tanpa menyentuh stok lagi.
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

        // Harga diambil dari database, bukan request (kalau client menentukan harga, siapa pun bisa membeli seharga nol); dari request hanya kanalnya.
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

    // Diskon tak boleh melebihi harga barang (juga CHECK di DB, migrasi 0015); di sini agar kasir mendapat kalimat yang terbaca.
    if diskon > subtotal {
        return Err(AppError::bad_request(
            "Diskon tidak boleh melebihi subtotal belanja.",
        ));
    }

    // Potongan Shopee: komisi + service_fee + PPh 0,5% dari omzet setelah diskon, plus Rp1.250 per transaksi, nol selain Shopee; `rupiah_bulat` sama strateginya dengan `Math.round` di kasir.
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

    // `subtotal` tetap harga barang; diskon dan ongkir punya kolom sendiri dan bertemu di `total_amount`; untuk Shopee `total_amount` adalah uang yang cair ke toko.
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

    // Baris produk sudah terkunci di transaksi ini, jadi penguncian di `kurangi` tak menunggu siapa pun; pemeriksaan stok tetap satu-satunya tempat stok berubah.
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

/// Membatalkan transaksi: stok kembali persis ke batch asal lalu ditandai `voided`; laporan omzet dan riwayat pelanggan sudah menyaring `status = 'completed'` sehingga tak perlu langkah tambahan.
pub async fn void_transaction(
    pool: &PgPool,
    transaction_id: Uuid,
    voided_by: Uuid,
    reason: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    let t = repo::kunci_transaksi(&mut tx, transaction_id)
        .await?
        .ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))?;
    if t.status != "completed" {
        return Err(AppError::conflict("Transaksi ini sudah dibatalkan."));
    }

    let pembalikan = repo::penyesuaian_penjualan(&mut tx, transaction_id).await?;
    if !pembalikan.is_empty() {
        stock::tambah(
            &mut tx,
            &pembalikan,
            StockReason::VoidReversal,
            transaction_id,
            Some(voided_by),
        )
        .await?;
    }

    repo::set_voided(&mut tx, transaction_id, voided_by, reason).await?;

    tx.commit().await?;
    Ok(())
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

    /// Bahan sidik jari paling biasa; tiap pengujian mengubah satu hal agar yang diuji benar-benar hal itu.
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
        // Rp45.000 × 17,25% = Rp7.762,50 tepat di tengah; `round_dp` default membulatkan ke genap (7762) sedangkan `Math.round` ke atas (7763), jadi uji ini mengunci arahnya.
        let basis = Decimal::new(45000, 0);
        let persen = Decimal::new(1725, 4); // 17,25%
        assert_eq!(rupiah_bulat(basis * persen), Decimal::new(7763, 0));

        // Kasus non-tengah tetap harus membulatkan seperti biasa.
        assert_eq!(rupiah_bulat(Decimal::new(72249, 2)), Decimal::new(722, 0));
    }

    #[test]
    fn skala_desimal_tidak_mengubah_sidik_jari() {
        // Nominal sama bisa tiba berskala beda (`70000` JSON vs `70000.00` NUMERIC); bila sidik jari berbeda, kirim ulang sah dijawab "isi berbeda".
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
        // Keranjang sama dengan ongkir berbeda adalah transaksi berbeda; bila sidik jari sama, kirim ulang setelah ongkir diperbaiki mengembalikan transaksi lama yang kurang nilainya.
        let items = [(produk(1), 1)];
        let dengan_ongkir = Bahan {
            shipping_cost: Decimal::new(20000, 0),
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&dengan_ongkir));
    }

    #[test]
    fn diskon_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Seperti ongkir, arah sebaliknya: tanpa diskon di sidik jari, kirim ulang menagih penuh dengan struk lama tanpa potongan.
        let items = [(produk(1), 1)];
        let dengan_diskon = Bahan {
            discount_amount: Decimal::new(5000, 0),
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&dengan_diskon));
    }

    #[test]
    fn persen_komisi_shopee_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Kasir bisa mengedit persen commission_fee per transaksi; tanpa ini di sidik jari, koreksi persen lalu kirim ulang dijawab transaksi lama berbiaya salah.
        let items = [(produk(1), 1)];
        let persen_beda = Bahan {
            platform_commission_fee_percent: Decimal::new(20, 2), // 20%
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&persen_beda));
    }

    #[test]
    fn persen_layanan_shopee_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Seperti komisi, untuk service_fee (program opsional yang persennya juga bisa diedit).
        let items = [(produk(1), 1)];
        let persen_beda = Bahan {
            platform_service_fee_percent: Decimal::new(5, 2), // 5%
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&persen_beda));
    }

    #[test]
    fn kanal_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Kanal berbeda memakai daftar harga berbeda sehingga tagihannya berbeda; tanpa kanal di sidik jari, membetulkan kanal lalu kirim ulang dijawab transaksi lama berharga salah.
        let items = [(produk(1), 1)];
        let shopee = Bahan {
            sales_channel: SalesChannel::Shopee,
            ..bahan(&items)
        };

        assert_ne!(sidik_jari(&bahan(&items)), sidik_jari(&shopee));
    }

    #[test]
    fn harga_kanal_jatuh_ke_harga_dasar_saat_belum_diatur() {
        // `None` berarti "belum diatur", bukan "gratis"; harga marketplace yang belum diisi adalah keadaan biasa dan menjual seharga nol adalah kerugian.
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
