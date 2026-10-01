//! Adapter Shopee Open API v2 dari spesifikasi `congminh1254/shopee-sdk` (MIT); beda dari TikTok: sukses = `error` kosong, order ditarik dua langkah, harga berupa angka.

pub mod auth;
pub mod client;
pub mod signature;

use super::{NormalizedOrder, NormalizedOrderItem, OrderStatus};
use rust_decimal::Decimal;
use serde::Deserialize;

/// Nama platform di tabel `platforms`, dibatasi CHECK `platforms_platform_name_check`.
pub const NAMA_PLATFORM: &str = "shopee";

/// Bentuk order Shopee; hanya field yang dipakai didaftarkan, sisanya utuh di `raw_payload`.
#[derive(Debug, Deserialize)]
pub struct ShopeeOrder {
    pub order_sn: String,
    pub order_status: String,
    #[serde(default)]
    pub total_amount: Option<f64>,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub shipping_carrier: Option<String>,
    #[serde(default)]
    pub item_list: Vec<ShopeeItem>,
}

#[derive(Debug, Deserialize)]
pub struct ShopeeItem {
    #[serde(default)]
    pub order_item_id: Option<i64>,
    #[serde(default)]
    pub item_name: Option<String>,
    #[serde(default)]
    pub model_sku: Option<String>,
    #[serde(default)]
    pub model_quantity_purchased: Option<i32>,
    #[serde(default)]
    pub model_discounted_price: Option<f64>,
}

/// `IN_CANCEL` dipetakan ke `New` (bukan `Processing`) agar order yang diminta batal tak ikut dipacking; status tak dikenal juga ke `New` agar tetap terlihat.
pub fn petakan_status(status: &str) -> OrderStatus {
    match status {
        "UNPAID" | "PENDING" | "INVOICE_PENDING" | "IN_CANCEL" => OrderStatus::New,
        "READY_TO_SHIP" | "PROCESSED" | "RETRY_SHIP" => OrderStatus::Processing,
        "SHIPPED" | "TO_CONFIRM_RECEIVE" => OrderStatus::Shipped,
        "COMPLETED" => OrderStatus::Completed,
        "CANCELLED" | "UNPAID_CANCELLED" => OrderStatus::Cancelled,
        _ => OrderStatus::New,
    }
}

/// Shopee mengirim uang sebagai angka JSON; lewat `f64` presisi hanya hilang di atas 2^53 (jauh di luar transaksi) dan `from_f64_retain` mempertahankan angkanya.
fn uang(nilai: Option<f64>) -> Option<Decimal> {
    nilai.and_then(Decimal::from_f64_retain)
}

/// Menerjemahkan order Shopee menjadi bentuk internal.
pub fn normalisasi(order: &ShopeeOrder, raw: serde_json::Value) -> NormalizedOrder {
    let items = order
        .item_list
        .iter()
        .map(|baris| NormalizedOrderItem {
            // `model_sku` adalah SKU penjual yang dikenali katalog AJ33; `order_item_id` hanya bila SKU kosong agar baris tetap punya rujukan.
            external_item_ref: baris
                .model_sku
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| baris.order_item_id.map(|id| id.to_string())),
            item_name: baris
                .item_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Produk tanpa nama".to_string()),
            // Jumlah hilang dianggap 1, bukan 0, karena baris ada berarti ada barangnya dan qty 0 membuat tiket meminta nol barang.
            qty: baris.model_quantity_purchased.unwrap_or(1).max(1),
            unit_price: uang(baris.model_discounted_price),
        })
        .collect();

    NormalizedOrder {
        external_order_id: order.order_sn.clone(),
        status: petakan_status(&order.order_status),
        total_amount: uang(order.total_amount),
        payment_method: order.payment_method.clone(),
        shipping_carrier: order.shipping_carrier.clone(),
        raw_payload: raw,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn order_json(item_list: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "order_sn": "2404098R48U37H",
            "order_status": "READY_TO_SHIP",
            "total_amount": 210000.0,
            "payment_method": "Cash on Delivery",
            "shipping_carrier": "J&T Express",
            "item_list": item_list
        })
    }

    fn normalisasi_dari(raw: serde_json::Value) -> NormalizedOrder {
        let order: ShopeeOrder = serde_json::from_value(raw.clone()).unwrap();
        normalisasi(&order, raw)
    }

    #[test]
    fn status_dipetakan_ke_istilah_internal() {
        assert_eq!(petakan_status("UNPAID"), OrderStatus::New);
        assert_eq!(petakan_status("READY_TO_SHIP"), OrderStatus::Processing);
        assert_eq!(petakan_status("PROCESSED"), OrderStatus::Processing);
        assert_eq!(petakan_status("SHIPPED"), OrderStatus::Shipped);
        assert_eq!(petakan_status("COMPLETED"), OrderStatus::Completed);
        assert_eq!(petakan_status("CANCELLED"), OrderStatus::Cancelled);
    }

    #[test]
    fn permintaan_batal_tidak_masuk_antrean_packing() {
        // Bila IN_CANCEL jatuh ke Processing, order yang diminta batal muncul di antrean packing dan bisa terlanjur dikirim.
        assert_eq!(petakan_status("IN_CANCEL"), OrderStatus::New);
        assert_ne!(petakan_status("IN_CANCEL"), OrderStatus::Processing);
    }

    #[test]
    fn status_asing_tetap_masuk_sebagai_baru() {
        assert_eq!(petakan_status("STATUS_YANG_BELUM_ADA"), OrderStatus::New);
    }

    #[test]
    fn qty_diambil_dari_model_quantity_purchased() {
        // Berbeda dari TikTok: satu baris mewakili beberapa unit sehingga tak ada yang perlu digabung.
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            {
                "order_item_id": 23620853561i64,
                "item_name": "Ayam Potong Utuh",
                "model_sku": "AY-UTUH-1KG",
                "model_quantity_purchased": 3,
                "model_discounted_price": 48000.0
            }
        ])));

        assert_eq!(hasil.items.len(), 1);
        assert_eq!(hasil.items[0].qty, 3);
        assert_eq!(
            hasil.items[0].external_item_ref.as_deref(),
            Some("AY-UTUH-1KG")
        );
    }

    #[test]
    fn qty_yang_hilang_tidak_menjadi_nol() {
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            { "order_item_id": 1i64, "item_name": "Ayam Fillet" }
        ])));

        assert_eq!(hasil.items[0].qty, 1);
    }

    #[test]
    fn sku_kosong_jatuh_ke_order_item_id() {
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            {
                "order_item_id": 23620853561i64,
                "item_name": "Ayam Fillet",
                "model_sku": "",
                "model_quantity_purchased": 1
            }
        ])));

        assert_eq!(
            hasil.items[0].external_item_ref.as_deref(),
            Some("23620853561")
        );
    }

    #[test]
    fn field_utama_ikut_terbawa() {
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            {
                "order_item_id": 1i64,
                "item_name": "Ayam Potong Utuh",
                "model_quantity_purchased": 1,
                "model_discounted_price": 48000.0
            }
        ])));

        assert_eq!(hasil.external_order_id, "2404098R48U37H");
        assert_eq!(hasil.status, OrderStatus::Processing);
        assert_eq!(
            hasil.total_amount,
            Some(Decimal::from_str("210000").unwrap())
        );
        assert_eq!(hasil.payment_method.as_deref(), Some("Cash on Delivery"));
        assert_eq!(hasil.shipping_carrier.as_deref(), Some("J&T Express"));
    }

    #[test]
    fn harga_pecahan_tidak_kehilangan_nilai() {
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            {
                "order_item_id": 1i64,
                "item_name": "Ayam Fillet",
                "model_quantity_purchased": 1,
                "model_discounted_price": 48500.5
            }
        ])));

        assert_eq!(
            hasil.items[0].unit_price,
            Some(Decimal::from_str("48500.5").unwrap())
        );
    }

    #[test]
    fn order_tanpa_item_tetap_bisa_dibaca() {
        // `get_order_list` tanpa `response_optional_fields` mengembalikan order tanpa `item_list`.
        let raw = serde_json::json!({
            "order_sn": "2404098R48U37H",
            "order_status": "CANCELLED"
        });
        let hasil = normalisasi_dari(raw);

        assert!(hasil.items.is_empty());
        assert_eq!(hasil.status, OrderStatus::Cancelled);
        assert!(hasil.total_amount.is_none());
    }

    #[test]
    fn nama_produk_kosong_diberi_penanda() {
        let hasil = normalisasi_dari(order_json(serde_json::json!([
            { "order_item_id": 1i64, "item_name": "", "model_quantity_purchased": 1 }
        ])));

        assert_eq!(hasil.items[0].item_name, "Produk tanpa nama");
    }

    #[test]
    fn sla_ditebak_dari_nama_kurir_shopee() {
        // Adapter hanya menyalin nama kurir; klasifikasi milik modul marketplace dan sama untuk semua platform.
        let hasil = normalisasi_dari(order_json(serde_json::json!([])));
        assert_eq!(
            crate::marketplace::klasifikasi_sla(hasil.shipping_carrier.as_deref()),
            crate::marketplace::SlaType::Reguler
        );
    }
}
