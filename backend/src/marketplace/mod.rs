//! Adapter (TikTok Shop, Shopee) menerjemahkan payload ke `NormalizedOrder`, satu-satunya bentuk yang masuk ke `orders`; tanpa trait karena pemanggil tahu platformnya.

pub mod crypto;
pub mod shopee;
pub mod tiktok;
pub mod token;

use chrono::{DateTime, Duration, Utc};

/// Status order setelah dipetakan ke istilah internal, dibatasi CHECK `external_orders_status_check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    New,
    Processing,
    Shipped,
    Completed,
    Cancelled,
}

impl OrderStatus {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "new" => Some(Self::New),
            "processing" => Some(Self::Processing),
            "shipped" => Some(Self::Shipped),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    fn peringkat(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Processing => 1,
            Self::Shipped => 2,
            Self::Completed => 3,
            Self::Cancelled => 4,
        }
    }

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

/// Status yang disimpan saat sinkron ulang: pesanan yang sudah bertiket atau lewat `new` hanya maju ke tahap sesudah pengiriman atau dibatalkan, tak pernah mundur.
pub fn status_setelah_sinkron(
    tersimpan: Option<(&str, bool)>,
    dari_platform: OrderStatus,
) -> OrderStatus {
    let Some((sekarang, ada_tiket)) = tersimpan else {
        return dari_platform;
    };
    let Some(sekarang) = OrderStatus::parse(sekarang) else {
        return dari_platform;
    };

    if dari_platform == OrderStatus::Cancelled {
        return OrderStatus::Cancelled;
    }
    if sekarang == OrderStatus::Cancelled {
        return sekarang;
    }
    // Tanpa tiket, `processing` hanya sisa pemetaan lama yang belum pernah dikerjakan, jadi boleh diikuti platform.
    if !ada_tiket && matches!(sekarang, OrderStatus::New | OrderStatus::Processing) {
        return dari_platform;
    }
    if matches!(dari_platform, OrderStatus::Shipped | OrderStatus::Completed)
        && dari_platform.peringkat() > sekarang.peringkat()
    {
        return dari_platform;
    }
    sekarang
}

/// Satu-satunya normalisasi nomor pesanan marketplace; kasir, label, dan sinkron harus memakainya agar nomor yang sama cocok.
pub fn normalkan_nomor_pesanan(raw: &str) -> String {
    raw.trim().to_uppercase()
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

/// Menebak SLA dari nama kurir adalah heuristik (platform tak punya metadata seragam); kurir tak dikenal jatuh ke `Reguler`, tenggat terlonggar, agar tak ada order salah ditandai mendesak.
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

/// Satu order dari platform mana pun dalam istilah internal, satu-satunya bentuk yang boleh menyeberang dari `marketplace` ke `orders`.
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
        // Jatuh ke tenggat terlonggar, karena salah menandai order biasa sebagai mendesak membuat antrean packing kehilangan arti.
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

    #[test]
    fn status_sinkron_mengikuti_platform_untuk_pesanan_baru_atau_tanpa_tiket() {
        assert_eq!(
            status_setelah_sinkron(None, OrderStatus::New),
            OrderStatus::New
        );
        assert_eq!(
            status_setelah_sinkron(Some(("new", false)), OrderStatus::Shipped),
            OrderStatus::Shipped
        );
        assert_eq!(
            status_setelah_sinkron(Some(("processing", false)), OrderStatus::New),
            OrderStatus::New
        );
    }

    #[test]
    fn status_sinkron_tidak_mundur_untuk_pesanan_bertiket_atau_selesai() {
        assert_eq!(
            status_setelah_sinkron(Some(("processing", true)), OrderStatus::New),
            OrderStatus::Processing
        );
        assert_eq!(
            status_setelah_sinkron(Some(("shipped", true)), OrderStatus::Processing),
            OrderStatus::Shipped
        );
        assert_eq!(
            status_setelah_sinkron(Some(("completed", false)), OrderStatus::New),
            OrderStatus::Completed
        );
    }

    #[test]
    fn status_sinkron_pembatalan_berlaku_dan_tak_dihidupkan_lagi() {
        assert_eq!(
            status_setelah_sinkron(Some(("processing", true)), OrderStatus::Cancelled),
            OrderStatus::Cancelled
        );
        assert_eq!(
            status_setelah_sinkron(Some(("cancelled", true)), OrderStatus::New),
            OrderStatus::Cancelled
        );
    }

    #[test]
    fn nomor_pesanan_dinormalkan_spasi_dan_huruf() {
        assert_eq!(normalkan_nomor_pesanan("  sp123abc "), "SP123ABC");
    }
}
