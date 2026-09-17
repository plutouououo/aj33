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
use rust_decimal::Decimal;
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
    pub amount_paid: Option<Decimal>,
    /// Ongkos kirim. `None` berarti nol -- bukan "tidak diketahui".
    pub shipping_cost: Option<Decimal>,
    pub items: Vec<CheckoutItem>,
    pub cashier_user_id: Uuid,
}

/// Uang selalu dibulatkan ke 2 angka di belakang koma sebelum disimpan,
/// supaya angka di struk tidak pernah meleset beberapa sen dari yang
/// tercatat.
fn uang(nilai: Decimal) -> Decimal {
    nilai.round_dp(2)
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

/// Sidik jari isi transaksi, untuk mendeteksi `Idempotency-Key` yang dipakai
/// ulang dengan isi yang BERBEDA.
///
/// Bahannya sengaja hanya yang ikut tersimpan di database, supaya sidik jari
/// transaksi lama bisa dihitung ulang dari barisnya -- tidak perlu kolom
/// khusus. Item diurutkan lebih dulu agar urutan input yang berbeda tapi
/// isinya sama tetap dianggap sama.
fn sidik_jari(
    transaction_type: TransactionType,
    customer_id: Option<Uuid>,
    payment_method: PaymentMethod,
    amount_paid: Option<Decimal>,
    shipping_cost: Decimal,
    items: &[(Uuid, i32)],
) -> String {
    let mut urut: Vec<(Uuid, i32)> = items.to_vec();
    urut.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(transaction_type.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(
        customer_id
            .map(|c| c.to_string())
            .unwrap_or_else(|| "null".into())
            .as_bytes(),
    );
    hasher.update(b"|");
    hasher.update(payment_method.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(sidik_uang(amount_paid).as_bytes());
    hasher.update(b"|");
    // Ongkir WAJIB ikut. Tanpa ini, kasir yang sadar ongkirnya belum terisi
    // lalu mengirim ulang keranjang yang sama dengan ongkir baru akan
    // dianggap mengirim permintaan kembar: backend mengembalikan transaksi
    // lama, layar menampilkan struk yang kurang sebesar ongkirnya, dan tidak
    // ada galat di mana pun yang memberi tahu.
    hasher.update(sidik_uang(Some(shipping_cost)).as_bytes());
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

    // Dinormalkan sekali di sini lalu dipakai untuk sidik jari MAUPUN total,
    // supaya angka yang di-hash persis angka yang tersimpan.
    let ongkir = uang(input.shipping_cost.unwrap_or(Decimal::ZERO));
    if ongkir.is_sign_negative() {
        return Err(AppError::bad_request("Ongkos kirim tidak boleh negatif."));
    }

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
    let sidik = sidik_jari(
        input.transaction_type,
        input.customer_id,
        input.payment_method,
        amount_paid_tersimpan,
        ongkir,
        &per_produk,
    );

    // Request ulang yang sah: kembalikan transaksi yang sudah ada, tanpa
    // menyentuh stok lagi.
    if let Some(existing) = repo::find_by_idempotency_key(pool, &input.idempotency_key).await? {
        let items_lama = repo::item_bahan_sidik_jari(pool, existing.id).await?;
        let sidik_lama = sidik_jari(
            parse_type(&existing.transaction_type)?,
            existing.customer_id,
            parse_payment(&existing.payment_method)?,
            existing.amount_paid,
            existing.shipping_cost,
            &items_lama,
        );

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
        // membeli apa saja seharga nol.
        let unit_price = uang(product.price);
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
    // `subtotal` tetap harga barang saja; ongkir hanya menambah total. Itulah
    // yang membuat margin masih bisa dihitung dari subtotal, dan sekaligus
    // membuat pemeriksaan uang tunai serta kembalian di bawah otomatis benar
    // tanpa disentuh -- keduanya sudah memakai `total_amount`.
    let total_amount = uang(subtotal + ongkir);

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
            subtotal,
            shipping_cost: ongkir,
            total_amount,
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

    #[test]
    fn urutan_item_tidak_mengubah_sidik_jari() {
        let a = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(10000, 0)),
            Decimal::ZERO,
            &[(produk(1), 2), (produk(2), 1)],
        );
        let b = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(10000, 0)),
            Decimal::ZERO,
            &[(produk(2), 1), (produk(1), 2)],
        );

        assert_eq!(a, b);
    }

    #[test]
    fn isi_yang_berbeda_menghasilkan_sidik_jari_berbeda() {
        let dasar = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(10000, 0)),
            Decimal::ZERO,
            &[(produk(1), 2)],
        );

        let qty_beda = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(10000, 0)),
            Decimal::ZERO,
            &[(produk(1), 3)],
        );
        assert_ne!(dasar, qty_beda);

        let bayar_beda = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(20000, 0)),
            Decimal::ZERO,
            &[(produk(1), 2)],
        );
        assert_ne!(dasar, bayar_beda);

        let metode_beda = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Transfer,
            Some(Decimal::new(10000, 0)),
            Decimal::ZERO,
            &[(produk(1), 2)],
        );
        assert_ne!(dasar, metode_beda);
    }

    #[test]
    fn pembulatan_uang_konsisten_dua_desimal() {
        assert_eq!(uang(Decimal::new(1005, 3)), Decimal::new(100, 2));
        assert_eq!(uang(Decimal::new(70000, 0)), Decimal::new(70000, 0));
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
        let dari_request = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(70000, 0)),
            Decimal::ZERO,
            &[(produk(1), 1)],
        );
        let dari_database = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(7000000, 2)),
            Decimal::ZERO,
            &[(produk(1), 1)],
        );

        assert_eq!(dari_request, dari_database);
    }

    #[test]
    fn ongkir_berbeda_menghasilkan_sidik_jari_berbeda() {
        // Keranjang yang sama dengan ongkir berbeda adalah transaksi yang
        // BERBEDA. Kalau sidik jarinya sama, mengirim ulang setelah ongkir
        // diperbaiki akan mengembalikan transaksi lama yang nilainya kurang --
        // tanpa galat apa pun yang memberi tahu.
        let tanpa_ongkir = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(70000, 0)),
            Decimal::ZERO,
            &[(produk(1), 1)],
        );
        let dengan_ongkir = sidik_jari(
            TransactionType::WalkIn,
            None,
            PaymentMethod::Cash,
            Some(Decimal::new(70000, 0)),
            Decimal::new(20000, 0),
            &[(produk(1), 1)],
        );

        assert_ne!(tanpa_ongkir, dengan_ongkir);
    }
}
