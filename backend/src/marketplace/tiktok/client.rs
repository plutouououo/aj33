//! Pemanggilan HTTP ke TikTok Shop Open API.

use super::signature;
use crate::config::TiktokConfig;
use crate::error::{AppError, AppResult};
use serde::Deserialize;

/// Bentuk jawaban baku TikTok Shop (`code`/`message` selalu ada, `data` hanya bila berhasil); bound ditulis sendiri karena derive serde menambah `T: Default` akibat `#[serde(default)]`.
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Amplop<T> {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct DaftarToko {
    #[serde(default)]
    shops: Vec<Toko>,
}

#[derive(Debug, Deserialize)]
struct Toko {
    cipher: String,
}

fn sekarang_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Menyusun URL lengkap beserta tanda tangan; parameter dirakit sekali dan dipakai untuk keduanya, karena dirakit dua kali membuat satu perubahan kecil menolak semua permintaan tanpa penjelasan.
fn url_bertanda_tangan(
    cfg: &TiktokConfig,
    path: &str,
    tambahan: &[(&str, String)],
    body: Option<&str>,
) -> String {
    let mut params: Vec<(String, String)> = vec![
        ("app_key".to_string(), cfg.app_key.clone()),
        ("timestamp".to_string(), sekarang_epoch().to_string()),
    ];
    for (k, v) in tambahan {
        params.push((k.to_string(), v.clone()));
    }

    let parameter = signature::susun_parameter(&params);
    let sign = signature::tanda_tangan(&cfg.app_secret, path, &parameter, body);
    params.push(("sign".to_string(), sign));

    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", escape(v)))
        .collect();

    format!(
        "{}{}?{}",
        cfg.host.trim_end_matches('/'),
        path,
        query.join("&")
    )
}

fn escape(nilai: &str) -> String {
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

async fn panggil<T: for<'de> Deserialize<'de>>(
    url: &str,
    access_token: &str,
) -> AppResult<Option<T>> {
    let res = reqwest::Client::new()
        .get(url)
        .header("x-tts-access-token", access_token)
        .send()
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "gagal menghubungi TikTok Shop");
            AppError::bad_request("Tidak bisa menghubungi TikTok Shop. Coba lagi sebentar lagi.")
        })?;

    let body: Amplop<T> = res.json().await.map_err(|err| {
        tracing::error!(error = %err, "jawaban TikTok Shop tidak bisa dibaca");
        AppError::bad_request("Jawaban dari TikTok Shop tidak dikenali.")
    })?;

    if body.code != 0 {
        return Err(AppError::bad_request(format!(
            "TikTok Shop menolak permintaan: {} ({})",
            body.message, body.code
        )));
    }

    Ok(body.data)
}

/// Mengambil toko pertama yang mengizinkan aplikasi: pembatasan disengaja karena AJ33 mengelola satu toko (banyak toko berarti `platforms` jadi satu baris per toko).
pub async fn toko_pertama_yang_diizinkan(
    cfg: &TiktokConfig,
    access_token: &str,
) -> AppResult<String> {
    let path = "/authorization/202309/shops";
    let url = url_bertanda_tangan(cfg, path, &[], None);

    let data: Option<DaftarToko> = panggil(&url, access_token).await?;

    data.and_then(|d| d.shops.into_iter().next())
        .map(|t| t.cipher)
        .ok_or_else(|| {
            AppError::bad_request(
                "Otorisasi berhasil, tapi TikTok tidak mengembalikan toko mana pun.",
            )
        })
}

#[derive(Debug, Deserialize)]
struct DetailOrder {
    #[serde(default)]
    orders: Vec<serde_json::Value>,
}

/// Detail satu order dari id-nya: webhook TikTok hanya membawa id, jadi detail diambil dari API agar data berasal dari sumber resmi, bukan isi webhook yang bisa tak lengkap.
pub async fn detail_order(
    cfg: &TiktokConfig,
    kredensial: &super::auth::Kredensial,
    order_id: &str,
) -> AppResult<Option<serde_json::Value>> {
    let path = "/order/202309/orders";
    let url = url_bertanda_tangan(
        cfg,
        path,
        &[
            ("shop_cipher", kredensial.shop_cipher.clone()),
            ("ids", order_id.to_string()),
        ],
        None,
    );

    let data: Option<DetailOrder> = panggil(&url, &kredensial.access_token).await?;

    Ok(data.and_then(|d| d.orders.into_iter().next()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> TiktokConfig {
        TiktokConfig {
            app_key: "kunci".into(),
            app_secret: "rahasia".into(),
            host: "https://open-api.test/".into(),
            auth_host: String::new(),
            service_id: String::new(),
            redirect_uri: String::new(),
        }
    }

    #[test]
    fn url_memuat_tanda_tangan_dan_parameter() {
        let url = url_bertanda_tangan(
            &cfg(),
            "/order/202309/orders",
            &[("ids", "123".into())],
            None,
        );

        assert!(url.starts_with("https://open-api.test/order/202309/orders?"));
        assert!(url.contains("app_key=kunci"));
        assert!(url.contains("ids=123"));
        assert!(url.contains("sign="));
        assert!(url.contains("timestamp="));
    }

    #[test]
    fn garis_miring_ganda_tidak_muncul_di_url() {
        // `host` dari .env sering ditulis dengan garis miring di akhir.
        let url = url_bertanda_tangan(&cfg(), "/order/202309/orders", &[], None);
        assert!(!url.contains("test//order"));
    }

    #[test]
    fn nilai_parameter_di_escape() {
        let url = url_bertanda_tangan(&cfg(), "/x", &[("ids", "a b&c".into())], None);
        assert!(url.contains("ids=a%20b%26c"));
    }
}
