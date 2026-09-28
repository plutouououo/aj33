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
// Tiga path di bawah milik bagian pengiriman, yang belum dipanggil dari
// mana pun -- lihat catatan di bagian "Memperbarui status pengiriman".
#[allow(dead_code)]
const PATH_PARAMETER_KIRIM: &str = "/api/v2/logistics/get_shipping_parameter";
#[allow(dead_code)]
const PATH_KIRIM_ORDER: &str = "/api/v2/logistics/ship_order";
#[allow(dead_code)]
const PATH_NOMOR_RESI: &str = "/api/v2/logistics/get_tracking_number";
// Belum dipanggil dari mana pun juga -- lihat catatan di bagian "Rincian
// akuntansi pesanan (escrow)".
#[allow(dead_code)]
const PATH_DETAIL_ESCROW: &str = "/api/v2/payment/get_escrow_detail";

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

// --- Menarik order ---

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

// --- Memperbarui status pengiriman ---
//
// BELUM TERSAMBUNG. Separuh pembaca order (`daftar_order`, `detail_order`)
// sudah dipakai; separuh pengiriman di bawah ini sudah ditulis terhadap
// spesifikasi Shopee tapi belum dipanggil route mana pun, dan belum ada
// tes yang menyentuhnya karena ketiganya memanggil HTTP langsung.
//
// Karena itu tiap itemnya diberi `#[allow(dead_code)]`: CI menjalankan
// `cargo clippy --all-targets -- -D warnings`, jadi tanpa ini seluruh
// bagian ini menggagalkan build. Atributnya sengaja dipasang per item,
// BUKAN sekali untuk seluruh berkas -- begitu bagian ini dipanggil dari
// `orders`, atribut yang tersisa langsung menunjuk apa yang masih
// menganggur. Satu `allow` di kepala berkas akan menyembunyikannya
// selamanya, termasuk kode mati yang benar-benar tidak sengaja.

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ParameterKirim {
    #[serde(default)]
    info_needed: InfoDiperlukan,
}

#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

#[allow(dead_code)]
fn terisi(daftar: &Option<Vec<String>>) -> bool {
    daftar.as_ref().is_some_and(|d| !d.is_empty())
}

/// Mengatur pengiriman satu order -- inilah yang memindahkan order dari
/// READY_TO_SHIP ke PROCESSED di Shopee.
///
/// Field `pickup`/`dropoff` tetap dikirim sebagai objek kosong walaupun
/// isinya tidak ada: dokumentasi `ship_order` menyebut field-nya harus tetap
/// ada, dan menghilangkannya ditolak.
#[allow(dead_code)]
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

#[allow(dead_code)]
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
#[allow(dead_code)]
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

// --- Rincian akuntansi pesanan (escrow) ---
//
// BELUM TERSAMBUNG, sama seperti bagian pengiriman di atas: `SHOPEE_PARTNER_ID`
// dkk. di `.env` masih kosong, jadi belum ada toko yang benar-benar
// tersambung untuk dipanggil. Disiapkan lebih dulu supaya begitu toko
// tersambung, tinggal dipanggil dari `orders` atau `reports` -- bukan
// ditulis dari nol saat kebutuhannya baru muncul.
//
// KENAPA ENDPOINT INI. Model biaya platform Shopee di `pos::service`
// (migrasi 0017/0018 -- `commission_fee`, `service_fee`, `withholding_tax`,
// `seller_order_processing_fee`) adalah PERKIRAAN yang kasir masukkan
// sendiri sebelum transaksi disimpan. `get_escrow_detail` adalah satu-
// satunya sumber angka SUNGGUHAN: laporan akuntansi resmi Shopee per
// pesanan, dibuat setelah pesanan selesai. Begitu toko tersambung, inilah
// yang dipanggil untuk membandingkan (atau menggantikan) perkiraan kasir
// dengan angka yang benar-benar Shopee potong.
//
// BENTUK RESPONSNYA ~80 FIELD (lihat skema `v2.payment.get_escrow_detail`
// di `congminh1254/shopee-sdk`); yang didaftarkan di `RincianEscrow`/
// `PendapatanOrder` cuma yang relevan untuk perbandingan itu. Field lain
// (pajak lintas-negara, kompensasi Shopee Ads, dst.) sengaja tidak
// didaftarkan -- kalau suatu saat perlu, tinggal ditambah, bukan ditulis
// ulang.

/// Bentuk jawaban `get_escrow_detail`. Hanya field tingkat atas yang dipakai
/// yang didaftarkan; sisanya (mis. `buyer_payment_info`) tidak diambil sama
/// sekali.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct RincianEscrow {
    #[serde(default)]
    pub order_sn: String,
    #[serde(default)]
    pub order_income: Option<PendapatanOrder>,
}

/// Subset `order_income` dari `get_escrow_detail` -- angka yang benar-benar
/// dibandingkan dengan perkiraan kasir di `pos::service::checkout`.
///
/// Semuanya `Option<f64>`, bukan `Decimal` atau wajib ada: dokumentasi
/// Shopee menyebut banyak field ini "Only display for non cb sip affiliate
/// shop", jadi ketidakhadirannya bukan kegagalan parsing.
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
pub struct PendapatanOrder {
    /// Uang yang sungguh cair ke toko untuk pesanan ini. Bandingkan dengan
    /// `total_amount` yang kasir catat di `pos::service::checkout`.
    #[serde(default)]
    pub escrow_amount: Option<f64>,
    #[serde(default)]
    pub buyer_total_amount: Option<f64>,
    /// "The commission fee charged by Shopee platform if applicable."
    /// Bandingkan dengan `platform_commission_fee` kasir.
    #[serde(default)]
    pub commission_fee: Option<f64>,
    /// "Amount charged by Shopee to seller for additional services"
    /// (mis. Gratis Ongkir Xtra, Star+). Bandingkan dengan
    /// `platform_service_fee` kasir.
    #[serde(default)]
    pub service_fee: Option<f64>,
    #[serde(default)]
    pub seller_transaction_fee: Option<f64>,
    /// "Cross-border tax imposed by the Indonesian government on sellers."
    /// TIDAK dimodelkan di `pos::service` -- lihat catatan perbandingan.
    #[serde(default)]
    pub escrow_tax: Option<f64>,
    /// "According to regulations issued by Directorate General of Taxation
    /// in ID, the Withholding Tax is applied to the income stated in the
    /// invoice..." -- PPh final UMKM. Bandingkan dengan
    /// `platform_withholding_tax` kasir.
    #[serde(default)]
    pub withholding_tax: Option<f64>,
    /// "Order Processing Fee is the amount charged to sellers for every
    /// order created." Bandingkan dengan `platform_order_processing_fee`
    /// kasir.
    #[serde(default)]
    pub seller_order_processing_fee: Option<f64>,
    #[serde(default)]
    pub actual_shipping_fee: Option<f64>,
    #[serde(default)]
    pub buyer_paid_shipping_fee: Option<f64>,
}

/// Mengambil rincian akuntansi satu pesanan.
///
/// Berbeda dari `detail_order`: dipanggil SETELAH pesanan selesai (angkanya
/// baru final saat itu), satu order per panggilan (Shopee juga punya
/// `get_escrow_detail_batch` untuk sampai 50 sekaligus, belum disiapkan di
/// sini karena belum ada pemanggil yang butuh itu).
#[allow(dead_code)]
pub async fn detail_escrow(
    cfg: &ShopeeConfig,
    kredensial: &Kredensial,
    order_sn: &str,
) -> AppResult<Option<RincianEscrow>> {
    let url = url_toko(
        cfg,
        kredensial,
        PATH_DETAIL_ESCROW,
        &[("order_sn", order_sn.to_string())],
    );

    kirim(reqwest::Client::new().get(&url)).await
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

    #[test]
    fn url_escrow_membawa_order_sn_dan_ditandatangani_seperti_order() {
        let url = url_toko(
            &cfg(),
            &kredensial(),
            PATH_DETAIL_ESCROW,
            &[("order_sn", "2404098R48U37H".into())],
        );

        assert!(url.starts_with("https://partner.test/api/v2/payment/get_escrow_detail?"));
        assert!(url.contains("order_sn=2404098R48U37H"));
        assert!(url.contains("access_token=token-akses"));
        assert!(url.contains("shop_id=322300222"));
        assert!(url.contains("sign="));
    }

    /// Potongan nyata dari skema `v2.payment.get_escrow_detail` (SDK
    /// `congminh1254/shopee-sdk`) -- memastikan `RincianEscrow` membaca field
    /// yang benar-benar dipakai untuk perbandingan, bukan salah ketik nama
    /// field yang baru ketahuan saat toko sungguhan tersambung.
    #[test]
    fn rincian_escrow_membaca_field_yang_dibandingkan_dengan_kasir() {
        let raw = serde_json::json!({
            "order_sn": "2404098R48U37H",
            "order_income": {
                "escrow_amount": 42875.0,
                "buyer_total_amount": 45000.0,
                "commission_fee": 776.25,
                "service_fee": 0.0,
                "seller_transaction_fee": 0.0,
                "escrow_tax": 0.0,
                "withholding_tax": 97.5,
                "seller_order_processing_fee": 1250.0,
                "actual_shipping_fee": 0.0,
                "buyer_paid_shipping_fee": 0.0
            }
        });

        let hasil: RincianEscrow = serde_json::from_value(raw).unwrap();
        let pendapatan = hasil.order_income.unwrap();

        assert_eq!(hasil.order_sn, "2404098R48U37H");
        assert_eq!(pendapatan.escrow_amount, Some(42875.0));
        assert_eq!(pendapatan.commission_fee, Some(776.25));
        assert_eq!(pendapatan.withholding_tax, Some(97.5));
        assert_eq!(pendapatan.seller_order_processing_fee, Some(1250.0));
    }

    #[test]
    fn rincian_escrow_field_yang_tidak_dikirim_shopee_tidak_gagal_parse() {
        // Banyak field `order_income` "hanya tampil untuk non cb sip
        // affiliate shop" menurut dokumentasi Shopee -- payload yang
        // memangkasnya harus tetap terbaca, bukan menolak seluruh respons.
        let raw = serde_json::json!({
            "order_sn": "2404098R48U37H",
            "order_income": {
                "escrow_amount": 42875.0
            }
        });

        let hasil: RincianEscrow = serde_json::from_value(raw).unwrap();
        let pendapatan = hasil.order_income.unwrap();

        assert_eq!(pendapatan.escrow_amount, Some(42875.0));
        assert_eq!(pendapatan.commission_fee, None);
    }
}
