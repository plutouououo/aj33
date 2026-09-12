//! Adapter TikTok Shop Open API.
//!
//! Sejak Tokopedia Open API dihentikan (migrasi wajib ke TikTok Shop
//! Partner Center per 30 September 2025), penjual Tokopedia dan TikTok Shop
//! memakai API yang sama. Jadi satu adapter ini melayani keduanya -- tidak
//! ada adapter Tokopedia terpisah.

pub mod auth;
pub mod client;
pub mod signature;

use super::{NormalizedOrder, NormalizedOrderItem, OrderStatus};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;

/// Nama platform di tabel `platforms`. Dibatasi CHECK constraint
/// `platforms_platform_name_check`.
pub const NAMA_PLATFORM: &str = "tiktok";

/// Bentuk order dari TikTok. Hanya field yang benar-benar dipakai yang
/// didaftarkan; sisanya tetap tersimpan utuh di `raw_payload`.
#[derive(Debug, Deserialize)]
pub struct TiktokOrder {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub payment: Option<TiktokPayment>,
    #[serde(default)]
    pub delivery_option_name: Option<String>,
    #[serde(default)]
    pub line_items: Vec<TiktokLineItem>,
}

#[derive(Debug, Deserialize)]
pub struct TiktokPayment {
    #[serde(default)]
    pub total_amount: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TiktokLineItem {
    pub id: String,
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub sale_price: Option<String>,
}

/// Memetakan status TikTok ke status internal.
///
/// Status yang tidak dikenal jatuh ke `New`, bukan diabaikan: order yang
/// statusnya asing tetap harus terlihat oleh Owner supaya bisa ditangani
/// manual, bukan hilang diam-diam.
pub fn petakan_status(status: &str) -> OrderStatus {
    match status {
        "UNPAID" | "ON_HOLD" => OrderStatus::New,
        "AWAITING_SHIPMENT" | "AWAITING_COLLECTION" => OrderStatus::Processing,
        "PARTIALLY_SHIPPING" | "IN_TRANSIT" => OrderStatus::Shipped,
        "DELIVERED" | "COMPLETED" => OrderStatus::Completed,
        "CANCELLED" => OrderStatus::Cancelled,
        _ => OrderStatus::New,
    }
}

fn uang(raw: Option<&String>) -> Option<Decimal> {
    raw.and_then(|s| Decimal::from_str(s).ok())
}

/// Menerjemahkan order TikTok menjadi bentuk internal.
///
/// CATATAN: tiap elemen `line_items` di TikTok mewakili SATU unit -- dua
/// barang yang sama muncul sebagai dua elemen dengan id berbeda. Karena itu
/// qty tiap baris selalu 1, dan jumlah sesungguhnya adalah banyaknya elemen.
/// Proyek lama menuliskan `qty: 1` dengan catatan "cek ulang di payload
/// asli"; di sini baris yang produknya sama digabung supaya jumlahnya benar.
pub fn normalisasi(order: &TiktokOrder, raw: serde_json::Value) -> NormalizedOrder {
    let mut items: Vec<NormalizedOrderItem> = Vec::new();

    for baris in &order.line_items {
        let nama = baris
            .product_name
            .clone()
            .unwrap_or_else(|| "Produk tanpa nama".to_string());
        let harga = uang(baris.sale_price.as_ref());

        match items
            .iter_mut()
            .find(|it| it.item_name == nama && it.unit_price == harga)
        {
            Some(ada) => ada.qty += 1,
            None => items.push(NormalizedOrderItem {
                external_item_ref: Some(baris.id.clone()),
                item_name: nama,
                qty: 1,
                unit_price: harga,
            }),
        }
    }

    NormalizedOrder {
        external_order_id: order.id.clone(),
        status: petakan_status(&order.status),
        total_amount: uang(order.payment.as_ref().and_then(|p| p.total_amount.as_ref())),
        payment_method: None,
        shipping_carrier: order.delivery_option_name.clone(),
        raw_payload: raw,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order_json(line_items: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": "5771234567890",
            "status": "AWAITING_SHIPMENT",
            "payment": { "total_amount": "210000.00" },
            "buyer_email": "pembeli@contoh.id",
            "delivery_option_name": "GrabExpress Instant",
            "line_items": line_items
        })
    }

    #[test]
    fn status_dipetakan_ke_istilah_internal() {
        assert_eq!(petakan_status("UNPAID"), OrderStatus::New);
        assert_eq!(petakan_status("AWAITING_SHIPMENT"), OrderStatus::Processing);
        assert_eq!(petakan_status("IN_TRANSIT"), OrderStatus::Shipped);
        assert_eq!(petakan_status("COMPLETED"), OrderStatus::Completed);
        assert_eq!(petakan_status("CANCELLED"), OrderStatus::Cancelled);
    }

    #[test]
    fn status_asing_tetap_masuk_sebagai_baru() {
        assert_eq!(petakan_status("STATUS_YANG_BELUM_ADA"), OrderStatus::New);
    }

    #[test]
    fn line_item_per_unit_digabung_jadi_qty() {
        // Tiga elemen untuk barang yang sama harus menjadi satu baris qty 3.
        // Kalau tidak, tiket packing meminta 1 barang padahal pembeli
        // memesan 3, dan stok berkurang kurang dari yang sebenarnya keluar.
        let raw = order_json(serde_json::json!([
            { "id": "a1", "product_name": "Totebag Kanvas", "sale_price": "70000.00" },
            { "id": "a2", "product_name": "Totebag Kanvas", "sale_price": "70000.00" },
            { "id": "a3", "product_name": "Totebag Kanvas", "sale_price": "70000.00" }
        ]));
        let order: TiktokOrder = serde_json::from_value(raw.clone()).unwrap();
        let hasil = normalisasi(&order, raw);

        assert_eq!(hasil.items.len(), 1);
        assert_eq!(hasil.items[0].qty, 3);
        assert_eq!(hasil.items[0].item_name, "Totebag Kanvas");
    }

    #[test]
    fn barang_berbeda_tetap_jadi_baris_sendiri() {
        let raw = order_json(serde_json::json!([
            { "id": "a1", "product_name": "Totebag Kanvas", "sale_price": "70000.00" },
            { "id": "b1", "product_name": "Mug Custom", "sale_price": "95000.00" }
        ]));
        let order: TiktokOrder = serde_json::from_value(raw.clone()).unwrap();
        let hasil = normalisasi(&order, raw);

        assert_eq!(hasil.items.len(), 2);
        assert!(hasil.items.iter().all(|i| i.qty == 1));
    }

    #[test]
    fn barang_sama_dengan_harga_berbeda_tidak_digabung() {
        // Harga berbeda berarti baris yang berbeda secara komersial
        // (misalnya satu kena promo), jadi tidak boleh dilebur.
        let raw = order_json(serde_json::json!([
            { "id": "a1", "product_name": "Mug Custom", "sale_price": "95000.00" },
            { "id": "a2", "product_name": "Mug Custom", "sale_price": "80000.00" }
        ]));
        let order: TiktokOrder = serde_json::from_value(raw.clone()).unwrap();
        let hasil = normalisasi(&order, raw);

        assert_eq!(hasil.items.len(), 2);
    }

    #[test]
    fn field_utama_ikut_terbawa() {
        let raw = order_json(serde_json::json!([
            { "id": "a1", "product_name": "Totebag Kanvas", "sale_price": "70000.00" }
        ]));
        let order: TiktokOrder = serde_json::from_value(raw.clone()).unwrap();
        let hasil = normalisasi(&order, raw);

        assert_eq!(hasil.external_order_id, "5771234567890");
        assert_eq!(hasil.status, OrderStatus::Processing);
        assert_eq!(
            hasil.total_amount,
            Some(Decimal::from_str("210000.00").unwrap())
        );
        assert_eq!(
            hasil.shipping_carrier.as_deref(),
            Some("GrabExpress Instant")
        );
    }

    #[test]
    fn payload_tanpa_line_items_tetap_bisa_dibaca() {
        // Beberapa event webhook hanya membawa perubahan status.
        let raw = serde_json::json!({ "id": "577", "status": "CANCELLED" });
        let order: TiktokOrder = serde_json::from_value(raw.clone()).unwrap();
        let hasil = normalisasi(&order, raw);

        assert!(hasil.items.is_empty());
        assert_eq!(hasil.status, OrderStatus::Cancelled);
    }
}
