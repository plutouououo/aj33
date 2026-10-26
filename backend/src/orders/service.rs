//! Pesanan Shopee dari label pengiriman: Astro membaca PDF-nya, di sini isinya diperiksa, SKU dicocokkan ke produk, lalu dicatat sebagai pesanan menunggu tiket.

use super::repo::{self, LabelBarisBaru, LabelBaru};
use crate::error::{AppError, AppResult};
use crate::marketplace::{shopee, SlaType};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PengirimanLabel {
    Instant,
    SameDay,
    Reguler,
}

impl PengirimanLabel {
    fn sla(self) -> SlaType {
        match self {
            Self::Instant => SlaType::Instant,
            Self::SameDay => SlaType::SameDay,
            Self::Reguler => SlaType::Reguler,
        }
    }

    fn kurir(self) -> &'static str {
        match self {
            Self::Instant => "SPX Instant",
            Self::SameDay => "SPX Sameday",
            Self::Reguler => "Reguler",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BarisLabel {
    pub name: String,
    pub sku: String,
    #[serde(default)]
    pub variation: Option<String>,
    pub qty: i32,
}

/// Isi satu label; nomor telepon tak ada karena Shopee menyembunyikan nomor penerima (yang tercetak milik pengirim).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelShopee {
    pub order_sn: String,
    pub recipient_name: String,
    pub address: String,
    pub city: String,
    pub district: String,
    /// Isi kotak pembayaran apa adanya, mis. "CASHLESS" atau "COD".
    pub payment_label: String,
    pub is_cod: bool,
    pub shipping: PengirimanLabel,
    #[serde(default)]
    pub ship_by: Option<NaiveDate>,
    #[serde(default)]
    pub pickup_code: Option<String>,
    pub items: Vec<BarisLabel>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusLabel {
    Dibuat,
    SudahAda,
    Gagal,
}

#[derive(Debug, Serialize)]
pub struct HasilLabel {
    pub order_sn: String,
    pub status: StatusLabel,
    pub order_id: Option<Uuid>,
    pub message: Option<String>,
}

/// Label diproses satu per satu dan gagalnya satu label tak membatalkan yang lain, karena satu PDF bisa berisi puluhan label.
pub async fn catat_label(pool: &PgPool, labels: Vec<LabelShopee>) -> AppResult<Vec<HasilLabel>> {
    if labels.is_empty() {
        return Err(AppError::bad_request("Tidak ada label untuk dicatat."));
    }

    let platform_id = repo::find_platform_id(pool, shopee::NAMA_PLATFORM)
        .await?
        .ok_or_else(|| AppError::internal("Platform Shopee belum terdaftar."))?;

    let mut hasil = Vec::with_capacity(labels.len());
    for label in labels {
        let order_sn = label.order_sn.trim().to_string();

        let siap = match periksa(&label) {
            Ok(siap) => siap,
            Err(pesan) => {
                hasil.push(gagal(order_sn, pesan));
                continue;
            }
        };

        let mut baris = Vec::with_capacity(siap.items.len());
        let mut sku_asing = Vec::new();
        for item in &siap.items {
            match repo::produk_dari_sku(pool, &item.sku).await? {
                Some(product_id) => baris.push(LabelBarisBaru {
                    product_id,
                    name: item.name.clone(),
                    sku: item.sku.clone(),
                    variation: item.variation.clone(),
                    qty: item.qty,
                }),
                None => sku_asing.push(item.sku.clone()),
            }
        }

        if !sku_asing.is_empty() {
            hasil.push(gagal(
                order_sn,
                format!(
                    "SKU berikut tidak ada di produk aktif: {}. Samakan SKU di Shopee dengan SKU di app.",
                    sku_asing.join(", ")
                ),
            ));
            continue;
        }

        let raw = serde_json::to_value(&label).map_err(|e| {
            tracing::error!(error = %e, "label tak bisa diserialisasi");
            AppError::internal("Label tidak bisa disimpan.")
        })?;

        let baru = LabelBaru {
            platform_id,
            order_sn: &siap.order_sn,
            sla_type: siap.shipping.sla().as_str(),
            sla_deadline: tenggat(siap.ship_by, siap.shipping),
            payment_label: &siap.payment_label,
            is_cod: siap.is_cod,
            shipping_carrier: siap.shipping.kurir(),
            recipient_name: &siap.recipient_name,
            address: &siap.address,
            city: &siap.city,
            district: &siap.district,
            pickup_code: siap.pickup_code.as_deref(),
            raw_payload: raw,
            items: &baris,
        };

        hasil.push(match repo::insert_pesanan_label(pool, &baru).await? {
            Some(id) => HasilLabel {
                order_sn: siap.order_sn,
                status: StatusLabel::Dibuat,
                order_id: Some(id),
                message: None,
            },
            None => HasilLabel {
                order_sn: siap.order_sn,
                status: StatusLabel::SudahAda,
                order_id: None,
                message: Some("Pesanan ini sudah pernah dicatat.".into()),
            },
        });
    }

    Ok(hasil)
}

fn gagal(order_sn: String, pesan: String) -> HasilLabel {
    HasilLabel {
        order_sn,
        status: StatusLabel::Gagal,
        order_id: None,
        message: Some(pesan),
    }
}

/// Batas kirim berlaku sampai akhir hari WIB; tanpa batas kirim dipakai tenggat bawaan SLA.
fn tenggat(ship_by: Option<NaiveDate>, shipping: PengirimanLabel) -> chrono::DateTime<Utc> {
    let wib = chrono::FixedOffset::east_opt(7 * 3600).expect("offset WIB sah");
    match ship_by.and_then(|d| {
        wib.from_local_datetime(&d.and_time(NaiveTime::from_hms_opt(23, 59, 59)?))
            .single()
    }) {
        Some(batas) => batas.with_timezone(&Utc),
        None => crate::marketplace::tenggat_sla(Utc::now(), shipping.sla()),
    }
}

/// Label yang sudah dirapikan; batas panjang mengikuti kolom tujuan agar galat database tak muncul sebagai 500.
#[derive(Debug)]
struct LabelSiap {
    order_sn: String,
    recipient_name: String,
    address: String,
    city: String,
    district: String,
    payment_label: String,
    is_cod: bool,
    shipping: PengirimanLabel,
    ship_by: Option<NaiveDate>,
    pickup_code: Option<String>,
    items: Vec<BarisLabel>,
}

fn teks(nilai: &str, nama: &str, maks: usize) -> Result<String, String> {
    let nilai = nilai.split_whitespace().collect::<Vec<_>>().join(" ");
    if nilai.is_empty() {
        return Err(format!("{nama} tidak terbaca dari label."));
    }
    if nilai.chars().count() > maks {
        return Err(format!("{nama} lebih dari {maks} karakter."));
    }
    Ok(nilai)
}

fn periksa(label: &LabelShopee) -> Result<LabelSiap, String> {
    let order_sn = teks(&label.order_sn, "Nomor pesanan", 100)?;
    if order_sn.contains(' ') {
        return Err("Nomor pesanan tidak boleh mengandung spasi.".into());
    }

    if label.items.is_empty() {
        return Err("Label tidak memuat barang.".into());
    }

    let mut items = Vec::with_capacity(label.items.len());
    for item in &label.items {
        let sku = teks(&item.sku, "SKU", 100)?.to_uppercase();
        if item.qty <= 0 {
            return Err(format!("Jumlah barang SKU {sku} harus lebih dari 0."));
        }
        let variation = match item.variation.as_deref().filter(|v| !v.trim().is_empty()) {
            Some(v) => Some(teks(v, "Variasi", 150)?),
            None => None,
        };
        items.push(BarisLabel {
            name: teks(&item.name, "Nama produk", 200)?,
            sku,
            variation,
            qty: item.qty,
        });
    }

    Ok(LabelSiap {
        order_sn,
        recipient_name: teks(&label.recipient_name, "Nama penerima", 150)?,
        address: teks(&label.address, "Alamat", 500)?,
        city: teks(&label.city, "Kabupaten/kota", 100)?,
        district: teks(&label.district, "Kecamatan", 100)?,
        payment_label: teks(&label.payment_label, "Metode pembayaran", 50)?,
        is_cod: label.is_cod,
        shipping: label.shipping,
        ship_by: label.ship_by,
        pickup_code: match label
            .pickup_code
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            Some(v) => Some(teks(v, "Kode pengambilan", 20)?),
            None => None,
        },
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label() -> LabelShopee {
        LabelShopee {
            order_sn: " 261018059B1MPR ".into(),
            recipient_name: "Juju".into(),
            address: "Jalan Ciptoning No. 40 b,  RT.1/RW.1, DEPOK,\nKAB. SLEMAN, DI YOGYAKARTA"
                .into(),
            city: "KAB. SLEMAN".into(),
            district: "DEPOK".into(),
            payment_label: "CASHLESS".into(),
            is_cod: false,
            shipping: PengirimanLabel::SameDay,
            ship_by: NaiveDate::from_ymd_opt(2026, 10, 19),
            pickup_code: Some(" 9WPS ".into()),
            items: vec![BarisLabel {
                name: "Ceker bersih kulit ada kuku".into(),
                sku: "cbsb-afc-1".into(),
                variation: None,
                qty: 1,
            }],
        }
    }

    #[test]
    fn spasi_dirapikan_dan_sku_dibesarkan() {
        let siap = periksa(&label()).unwrap();
        assert_eq!(siap.order_sn, "261018059B1MPR");
        assert_eq!(
            siap.address,
            "Jalan Ciptoning No. 40 b, RT.1/RW.1, DEPOK, KAB. SLEMAN, DI YOGYAKARTA"
        );
        assert_eq!(siap.items[0].sku, "CBSB-AFC-1");
        assert_eq!(siap.pickup_code.as_deref(), Some("9WPS"));
    }

    #[test]
    fn label_tanpa_barang_atau_sku_ditolak() {
        let tanpa_barang = LabelShopee {
            items: vec![],
            ..label()
        };
        assert!(periksa(&tanpa_barang).is_err());

        let mut tanpa_sku = label();
        tanpa_sku.items[0].sku = "  ".into();
        assert!(periksa(&tanpa_sku).is_err());
    }

    #[test]
    fn kode_pengambilan_kosong_tak_disimpan() {
        let kosong = LabelShopee {
            pickup_code: Some("  ".into()),
            ..label()
        };
        assert_eq!(periksa(&kosong).unwrap().pickup_code, None);
    }

    #[test]
    fn jumlah_nol_ditolak() {
        let mut nol = label();
        nol.items[0].qty = 0;
        assert!(periksa(&nol).is_err());
    }

    #[test]
    fn batas_kirim_berakhir_tengah_malam_wib() {
        let batas = tenggat(
            NaiveDate::from_ymd_opt(2026, 10, 19),
            PengirimanLabel::SameDay,
        );
        assert_eq!(batas.to_rfc3339(), "2026-10-19T16:59:59+00:00");
    }
}
