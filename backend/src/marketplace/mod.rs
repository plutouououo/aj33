//! Integrasi marketplace.
//!
//! Bentuk order tiap platform berbeda-beda. Modul `orders` tidak boleh tahu
//! bentuk aslinya: adapter yang menerjemahkan payload platform menjadi
//! `NormalizedOrder`, dan hanya bentuk itu yang masuk ke basis data.
//!
//! Sekarang ada dua adapter: TikTok Shop dan Shopee. Tetap belum ada trait
//! `PlatformAdapter` dengan dispatch dinamis seperti di proyek lama, karena
//! yang memanggil adapter selalu tahu platform mana yang dimaksud -- route
//! `/platforms/shopee/...` tidak pernah perlu memilih adapter saat runtime.
//! Yang menjaga modularitas tetap `NormalizedOrder`: kedua adapter bermuara
//! ke bentuk itu, dan hanya bentuk itu yang masuk ke modul `orders`.
//!
//! Yang benar-benar sama antar keduanya sudah dipisah: `crypto` untuk
//! enkripsi token, dan `token` untuk membaca/menulisnya di tabel
//! `platforms`.

pub mod crypto;
pub mod shopee;
pub mod tiktok;
pub mod token;

use chrono::{DateTime, Duration, Utc};

/// Status order setelah dipetakan ke istilah internal. Nilainya dibatasi
/// CHECK constraint `external_orders_status_check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    New,
    Processing,
    Shipped,
    Completed,
    Cancelled,
}

impl OrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Processing => "processing",
            Self::Shipped => "shipped",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Tingkat layanan pengiriman, menentukan tenggat packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlaType {
    Instant,
    SameDay,
    Reguler,
}

impl SlaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::SameDay => "same_day",
            Self::Reguler => "reguler",
        }
    }

    fn jam(self) -> i64 {
        match self {
            Self::Instant => 3,
            Self::SameDay => 6,
            Self::Reguler => 48,
        }
    }
}

/// Menebak SLA dari nama kurir.
///
/// Ini heuristik, bukan data resmi: platform belum menyediakan metadata
/// tingkat layanan yang seragam. Konsekuensinya tenggat packing bisa
/// meleset untuk kurir yang namanya tidak dikenali -- dan yang tidak
/// dikenali jatuh ke `Reguler`, tenggat paling longgar, supaya tidak ada
/// order yang salah ditandai mendesak.
pub fn klasifikasi_sla(shipping_carrier: Option<&str>) -> SlaType {
    let nama = shipping_carrier.unwrap_or_default().to_lowercase();

    if ["instant", "grabexpress"].iter().any(|k| nama.contains(k)) {
        SlaType::Instant
    } else if ["same day", "sameday", "same_day"]
        .iter()
        .any(|k| nama.contains(k))
    {
        SlaType::SameDay
    } else {
        SlaType::Reguler
    }
}

pub fn tenggat_sla(diterima: DateTime<Utc>, sla: SlaType) -> DateTime<Utc> {
    diterima + Duration::hours(sla.jam())
}

#[derive(Debug, Clone)]
pub struct NormalizedOrderItem {
    pub external_item_ref: Option<String>,
    pub item_name: String,
    pub qty: i32,
    pub unit_price: Option<rust_decimal::Decimal>,
}

/// Satu order dari platform mana pun, sudah diterjemahkan ke istilah
/// internal. Ini satu-satunya bentuk yang boleh menyeberang dari modul
/// marketplace ke modul `orders`.
#[derive(Debug, Clone)]
pub struct NormalizedOrder {
    pub external_order_id: String,
    pub status: OrderStatus,
    pub total_amount: Option<rust_decimal::Decimal>,
    pub payment_method: Option<String>,
    pub shipping_carrier: Option<String>,
    pub raw_payload: serde_json::Value,
    pub items: Vec<NormalizedOrderItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kurir_instant_dikenali() {
        assert_eq!(
            klasifikasi_sla(Some("GrabExpress Instant")),
            SlaType::Instant
        );
        assert_eq!(klasifikasi_sla(Some("Instant Courier")), SlaType::Instant);
    }

    #[test]
    fn kurir_same_day_dikenali() {
        assert_eq!(klasifikasi_sla(Some("JNE Same Day")), SlaType::SameDay);
        assert_eq!(klasifikasi_sla(Some("sameday")), SlaType::SameDay);
    }

    #[test]
    fn kurir_tak_dikenal_jatuh_ke_reguler() {
        // Jatuh ke tenggat paling longgar, bukan paling ketat: salah
        // menandai order biasa sebagai mendesak membuat antrean packing
        // kehilangan arti.
        assert_eq!(
            klasifikasi_sla(Some("Kurir Antah Berantah")),
            SlaType::Reguler
        );
        assert_eq!(klasifikasi_sla(None), SlaType::Reguler);
    }

    #[test]
    fn tenggat_dihitung_dari_waktu_diterima() {
        let diterima = DateTime::parse_from_rfc3339("2026-09-11T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            tenggat_sla(diterima, SlaType::Instant).to_rfc3339(),
            "2026-09-11T13:00:00+00:00"
        );
        assert_eq!(
            tenggat_sla(diterima, SlaType::Reguler).to_rfc3339(),
            "2026-09-13T10:00:00+00:00"
        );
    }
}
