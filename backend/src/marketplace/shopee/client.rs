//! Pemanggilan HTTP ke Shopee Open API v2.
//!
//! Satu aturan yang menentukan bentuk modul ini: path yang ditandatangani
//! harus PERSIS path yang diminta. Karena itu path lengkap (`/api/v2/...`)
//! ditulis utuh di tiap pemanggil dan dipakai untuk keduanya sekaligus --
//! tidak ada `base_url` berisi `/api/v2` yang harus digabung ulang saat
//! menandatangani, karena di situlah selisih satu segmen bisa menyelinap
//! dan menghasilkan `error_sign` yang tidak menjelaskan apa-apa.

use super::auth::Kredensial;
use super::signature;
use crate::config::ShopeeConfig;
use crate::error::{AppError, AppResult};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const PATH_DAFTAR_ORDER: &str = "/api/v2/order/get_order_list";
const PATH_DETAIL_ORDER: &str = "/api/v2/order/get_order_detail";
const PATH_PARAMETER_KIRIM: &str = "/api/v2/logistics/get_shipping_parameter";
const PATH_KIRIM_ORDER: &str = "/api/v2/logistics/ship_order";
const PATH_NOMOR_RESI: &str = "/api/v2/logistics/get_tracking_number";

/// Batas `page_size` menurut dokumentasi `get_order_list`.
const MAKS_PER_HALAMAN: i32 = 100;

/// Batas `order_sn_list` menurut dokumentasi `get_order_detail`.
pub const MAKS_DETAIL_SEKALI_MINTA: usize = 50;

/// Rentang waktu terlebar yang diterima `get_order_list`, dalam detik
/// (15 hari). Permintaan yang lebih lebar ditolak Shopee.
const MAKS_RENTANG_DETIK: i64 = 15 * 24 * 60 * 60;

/// Bentuk jawaban baku Shopee: sukses ditandai `error` yang KOSONG, bukan
/// kode angka. Isi sebenarnya ada di `response`.
///
/// Bound dituliskan sendiri karena derive serde akan menambahkan
/// `T: Default` gara-gara `#[serde(default)]` di `response` -- padahal yang
/// diberi nilai default adalah `Option<T>`, bukan `T`.
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Amplop<T> {
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    response: Option<T>,
}

fn sekarang_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Peng-escape-an nilai query string. Sama seperti di adapter TikTok.
pub fn escape(nilai: &str) -> String {
    nilai
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn rakit_url(host: &str, path: &str, params: &[(&str, String)]) -> String {
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", escape(v)))
        .collect();

    format!("{}{}?{}", host.trim_end_matches('/'), path, query.join("&"))
}

/// URL endpoint publik: hanya `partner_id`, `timestamp`, dan `sign`.
pub fn url_publik(cfg: &ShopeeConfig, path: &str) -> String {
    let timestamp = sekarang_epoch();
    let sign = signature::tanda_tangan_publik(&cfg.partner_key, cfg.partner_id, path, timestamp);

    rakit_url(
        &cfg.host,
        path,
        &[
            ("partner_id", cfg.partner_id.to_string()),
            ("timestamp", timestamp.to_string()),
            ("sign", sign),
        ],
    )
}

/// URL endpoint level toko: menambahkan `access_token` dan `shop_id`, yang
/// juga ikut ditandatangani.
fn url_toko(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    path: &str,
    tambahan: &[(&str, String)],
) -> String {
    let timestamp = sekarang_epoch();
    let sign = signature::tanda_tangan_toko(
        &cfg.partner_key,
        cfg.partner_id,
        path,
        timestamp,
        &kredensial.access_token,
        kredensial.shop_id,
    );

    let mut params: Vec<(&str, String)> = vec![
        ("partner_id", cfg.partner_id.to_string()),
        ("timestamp", timestamp.to_string()),
        ("access_token", kredensial.access_token.clone()),
        ("shop_id", kredensial.shop_id.to_string()),
        ("sign", sign),
    ];
    params.extend(tambahan.iter().cloned());

    rakit_url(&cfg.host, path, &params)
}

async fn kirim<T: DeserializeOwned>(permintaan: reqwest::RequestBuilder) -> AppResult<Option<T>> {
    let res = permintaan.send().await.map_err(|err| {
        tracing::error!(error = %err, "gagal menghubungi Shopee");
        AppError::bad_request("Tidak bisa menghubungi Shopee. Coba lagi sebentar lagi.")
    })?;

    let body: Amplop<T> = res.json().await.map_err(|err| {
        tracing::error!(error = %err, "jawaban Shopee tidak bisa dibaca");
        AppError::bad_request("Jawaban dari Shopee tidak dikenali.")
    })?;

    if !body.error.is_empty() {
        return Err(AppError::bad_request(format!(
            "Shopee menolak permintaan: {} ({})",
            body.message, body.error
        )));
    }

    Ok(body.response)
}

// ---------------------------------------------------------------------
// Menarik order
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DaftarOrder {
    #[serde(default)]
    order_list: Vec<RingkasanOrder>,
    #[serde(default)]
    more: bool,
    #[serde(default)]
    next_cursor: String,
}

#[derive(Debug, Deserialize)]
struct RingkasanOrder {
    order_sn: String,
}

/// Mengambil semua `order_sn` dalam satu rentang waktu.
///
/// `get_order_list` hanya mengembalikan nomor order, bukan isinya -- jadi
/// hasilnya diumpankan ke `detail_order`. Pemisahan itu memang bentuk API
/// Shopee, bukan pilihan kita.
///
/// Rentangnya dibatasi 15 hari oleh Shopee, dan itu diperiksa di sini
/// supaya penyebabnya jelas di log kita sendiri, bukan muncul sebagai
/// `error_param` yang tidak menyebut batasnya.
pub async fn daftar_order(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    sejak: i64,
    sampai: i64,
    status: Option<&str>,
) -> AppResult<Vec<String>> {
    if sampai < sejak {
        return Err(AppError::bad_request(
            "Rentang waktu penarikan order terbalik.",
        ));
    }
    if sampai - sejak > MAKS_RENTANG_DETIK {
        return Err(AppError::bad_request(
            "Rentang penarikan order Shopee paling lebar 15 hari.",
        ));
    }

    let mut hasil = Vec::new();
    let mut cursor = String::new();

    loop {
        let mut params: Vec<(&str, String)> = vec![
            ("time_range_field", "update_time".to_string()),
            ("time_from", sejak.to_string()),
            ("time_to", sampai.to_string()),
            ("page_size", MAKS_PER_HALAMAN.to_string()),
        ];
        if !cursor.is_empty() {
            params.push(("cursor", cursor.clone()));
        }
        if let Some(status) = status {
            params.push(("order_status", status.to_string()));
        }

        let url = url_toko(cfg, kredensial, PATH_DAFTAR_ORDER, &params);
        let data: Option<DaftarOrder> = kirim(reqwest::Client::new().get(&url)).await?;

        let Some(data) = data else { break };

        hasil.extend(data.order_list.into_iter().map(|o| o.order_sn));

        // Cursor kosong dengan `more` true berarti halaman berikutnya tidak
        // bisa ditentukan; berhenti daripada meminta halaman yang sama
        // berulang kali.
        if !data.more || data.next_cursor.is_empty() {
            break;
        }
        cursor = data.next_cursor;
    }

    Ok(hasil)
}

#[derive(Debug, Deserialize)]
struct DetailOrder {
    #[serde(default)]
    order_list: Vec<serde_json::Value>,
}

/// Mengambil detail beberapa order sekaligus, paling banyak 50 per panggilan.
///
/// Dikembalikan sebagai JSON mentah supaya `normalisasi` yang memutuskan
/// field mana yang dipakai, dan sisanya tetap utuh di `raw_payload`.
pub async fn detail_order(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    order_sn: &[String],
) -> AppResult<Vec<serde_json::Value>> {
    if order_sn.is_empty() {
        return Ok(Vec::new());
    }
    if order_sn.len() > MAKS_DETAIL_SEKALI_MINTA {
        return Err(AppError::bad_request(format!(
            "Detail order Shopee paling banyak {MAKS_DETAIL_SEKALI_MINTA} sekali minta."
        )));
    }

    let url = url_toko(
        cfg,
        kredensial,
        PATH_DETAIL_ORDER,
        &[
            ("order_sn_list", order_sn.join(",")),
            // Tanpa ini Shopee hanya mengembalikan sedikit field, dan
            // `item_list` -- yang dipakai membuat tiket packing -- tidak
            // termasuk di dalamnya.
            (
                "response_optional_fields",
                "item_list,recipient_address,payment_method,total_amount,shipping_carrier,package_list,order_status,pay_time,ship_by_date"
                    .to_string(),
            ),
        ],
    );

    let data: Option<DetailOrder> = kirim(reqwest::Client::new().get(&url)).await?;

    Ok(data.map(|d| d.order_list).unwrap_or_default())
}

// ---------------------------------------------------------------------
// Memperbarui status pengiriman
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ParameterKirim {
    #[serde(default)]
    info_needed: InfoDiperlukan,
}

#[derive(Debug, Default, Deserialize)]
struct InfoDiperlukan {
    #[serde(default)]
    pickup: Option<Vec<String>>,
    #[serde(default)]
    dropoff: Option<Vec<String>>,
}

/// Cara pengiriman yang diminta Shopee untuk satu order.
///
/// Bukan pilihan kita: tiap kurir menentukan sendiri apakah paket dijemput
/// atau diantar ke titik drop-off, dan mengirim bentuk yang salah ditolak.
/// Karena itu bentuknya selalu ditanyakan dulu lewat `parameter_pengiriman`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetodeKirim {
    /// Kurir menjemput ke alamat penjual.
    Pickup,
    /// Penjual mengantar ke titik drop-off.
    Dropoff,
}

/// Menanyakan bentuk pengiriman yang diminta untuk satu order.
///
/// `info_needed` yang kosong di kedua sisi berarti Shopee tidak menyebut
/// cara mana pun -- biasanya karena ordernya belum siap dikirim. Itu
/// dikembalikan sebagai `None` supaya pemanggil bisa membedakannya dari
/// order yang memang perlu dijemput.
pub async fn parameter_pengiriman(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    order_sn: &str,
) -> AppResult<Option<MetodeKirim>> {
    let url = url_toko(
        cfg,
        kredensial,
        PATH_PARAMETER_KIRIM,
        &[("order_sn", order_sn.to_string())],
    );

    let data: Option<ParameterKirim> = kirim(reqwest::Client::new().get(&url)).await?;

    let Some(info) = data.map(|d| d.info_needed) else {
        return Ok(None);
    };

    // Shopee mengirim daftar kosong (bukan field yang hilang) untuk cara
    // yang tidak berlaku, jadi yang menentukan adalah daftar yang ADA ISINYA.
    Ok(match (terisi(&info.pickup), terisi(&info.dropoff)) {
        (true, _) => Some(MetodeKirim::Pickup),
        (_, true) => Some(MetodeKirim::Dropoff),
        _ => None,
    })
}

fn terisi(daftar: &Option<Vec<String>>) -> bool {
    daftar.as_ref().is_some_and(|d| !d.is_empty())
}

/// Mengatur pengiriman satu order -- inilah yang memindahkan order dari
/// READY_TO_SHIP ke PROCESSED di Shopee.
///
/// Field `pickup`/`dropoff` tetap dikirim sebagai objek kosong walaupun
/// isinya tidak ada: dokumentasi `ship_order` menyebut field-nya harus tetap
/// ada, dan menghilangkannya ditolak.
pub async fn kirim_order(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    order_sn: &str,
    metode: MetodeKirim,
) -> AppResult<()> {
    let body = match metode {
        MetodeKirim::Pickup => serde_json::json!({
            "order_sn": order_sn,
            "pickup": {},
        }),
        MetodeKirim::Dropoff => serde_json::json!({
            "order_sn": order_sn,
            "dropoff": {},
        }),
    };

    let url = url_toko(cfg, kredensial, PATH_KIRIM_ORDER, &[]);
    let _: Option<serde_json::Value> = kirim(reqwest::Client::new().post(&url).json(&body)).await?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct NomorResi {
    #[serde(default)]
    tracking_number: Option<String>,
}

/// Mengambil nomor resi setelah pengiriman diatur.
///
/// Dipisah dari `kirim_order` karena memang terbit belakangan: sebagian
/// kurir baru menerbitkan resi beberapa saat setelah pengiriman diatur,
/// jadi resi yang belum ada bukan kegagalan -- itu `None`.
pub async fn nomor_resi(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    order_sn: &str,
) -> AppResult<Option<String>> {
    let url = url_toko(
        cfg,
        kredensial,
        PATH_NOMOR_RESI,
        &[("order_sn", order_sn.to_string())],
    );

    let data: Option<NomorResi> = kirim(reqwest::Client::new().get(&url)).await?;

    Ok(data
        .and_then(|d| d.tracking_number)
        .filter(|n| !n.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ShopeeConfig {
        ShopeeConfig {
            partner_id: 1001141,
            partner_key: "rahasia".into(),
            host: "https://partner.test/".into(),
            auth_url: String::new(),
            redirect_uri: String::new(),
        }
    }

    fn kredensial() -> Kredensial {
        Kredensial {
            shop_id: 322300222,
            access_token: "token-akses".into(),
        }
    }

    #[test]
    fn url_publik_memuat_parameter_wajib() {
        let url = url_publik(&cfg(), "/api/v2/auth/token/get");

        assert!(url.starts_with("https://partner.test/api/v2/auth/token/get?"));
        assert!(url.contains("partner_id=1001141"));
        assert!(url.contains("timestamp="));
        assert!(url.contains("sign="));
    }

    #[test]
    fn url_publik_tidak_membawa_access_token() {
        // Endpoint token dipanggil justru karena belum punya token.
        let url = url_publik(&cfg(), "/api/v2/auth/token/get");
        assert!(!url.contains("access_token"));
        assert!(!url.contains("shop_id"));
    }

    #[test]
    fn url_toko_membawa_token_dan_shop_id() {
        let url = url_toko(
            &cfg(),
            &kredensial(),
            PATH_DAFTAR_ORDER,
            &[("page_size", "100".into())],
        );

        assert!(url.starts_with("https://partner.test/api/v2/order/get_order_list?"));
        assert!(url.contains("access_token=token-akses"));
        assert!(url.contains("shop_id=322300222"));
        assert!(url.contains("page_size=100"));
        assert!(url.contains("sign="));
    }

    #[test]
    fn garis_miring_ganda_tidak_muncul_di_url() {
        // `host` dari .env sering ditulis dengan garis miring di akhir.
        let url = url_toko(&cfg(), &kredensial(), PATH_DAFTAR_ORDER, &[]);
        assert!(!url.contains("test//api"));
    }

    #[test]
    fn nilai_parameter_di_escape() {
        let url = url_toko(
            &cfg(),
            &kredensial(),
            PATH_DETAIL_ORDER,
            &[("order_sn_list", "A1,B2".into())],
        );
        assert!(url.contains("order_sn_list=A1%2CB2"));
    }

    #[test]
    fn tanda_tangan_url_toko_cocok_dengan_yang_dihitung_ulang() {
        // Tanda tangan dan parameter dirakit dari sumber yang sama; kalau
        // suatu saat dipisah, tes ini yang gagal lebih dulu.
        let url = url_toko(&cfg(), &kredensial(), PATH_DAFTAR_ORDER, &[]);

        let timestamp: i64 = potong(&url, "timestamp=").parse().unwrap();
        let sign = potong(&url, "sign=");

        assert_eq!(
            sign,
            signature::tanda_tangan_toko(
                "rahasia",
                1001141,
                PATH_DAFTAR_ORDER,
                timestamp,
                "token-akses",
                322300222,
            )
        );
    }

    fn potong(url: &str, kunci: &str) -> String {
        url.split(kunci)
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn info_needed_kosong_berarti_belum_ada_cara_kirim() {
        let info = InfoDiperlukan {
            pickup: Some(vec![]),
            dropoff: Some(vec![]),
        };
        assert!(!terisi(&info.pickup));
        assert!(!terisi(&info.dropoff));
    }

    #[test]
    fn daftar_yang_ada_isinya_menentukan_cara_kirim() {
        let info = InfoDiperlukan {
            pickup: Some(vec!["address_id".into(), "pickup_time_id".into()]),
            dropoff: Some(vec![]),
        };
        assert!(terisi(&info.pickup));
        assert!(!terisi(&info.dropoff));
    }

    #[tokio::test]
    async fn rentang_lebih_dari_15_hari_ditolak_sebelum_dikirim() {
        let sejak = 1_700_000_000;
        let terlalu_lebar = sejak + MAKS_RENTANG_DETIK + 1;

        let hasil = daftar_order(&cfg(), &kredensial(), sejak, terlalu_lebar, None).await;
        assert!(hasil.is_err());
    }

    #[tokio::test]
    async fn rentang_terbalik_ditolak() {
        let hasil = daftar_order(&cfg(), &kredensial(), 1_700_000_000, 1_600_000_000, None).await;
        assert!(hasil.is_err());
    }

    #[tokio::test]
    async fn detail_tanpa_order_tidak_memanggil_shopee() {
        // Kalau ini sampai memanggil jaringan, tesnya yang gagal duluan --
        // host `partner.test` tidak ada.
        let hasil = detail_order(&cfg(), &kredensial(), &[]).await.unwrap();
        assert!(hasil.is_empty());
    }

    #[tokio::test]
    async fn detail_lebih_dari_50_order_ditolak() {
        let terlalu_banyak: Vec<String> = (0..51).map(|i| format!("SN{i}")).collect();
        let hasil = detail_order(&cfg(), &kredensial(), &terlalu_banyak).await;
        assert!(hasil.is_err());
    }
}
