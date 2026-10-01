//! Otorisasi toko Shopee: callback membawa `code` dan `shop_id` (wajib); refresh token sekali pakai, jadi hasil perpanjangan harus selalu disimpan.

use super::client;
use super::NAMA_PLATFORM;
use crate::config::ShopeeConfig;
use crate::error::{AppError, AppResult};
use crate::marketplace::token;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;

/// Nama platform untuk pesan yang dibaca Owner.
const NAMA_TAMPILAN: &str = "Shopee";

const PATH_TUKAR_KODE: &str = "/api/v2/auth/token/get";
const PATH_PERBARUI: &str = "/api/v2/auth/access_token/get";

/// Kredensial siap pakai untuk memanggil endpoint level toko.
pub struct Kredensial {
    pub shop_id: i64,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expire_in: Option<i64>,
}

struct TokenBaru {
    access_token: String,
    refresh_token: String,
    kedaluwarsa: DateTime<Utc>,
}

/// URL otorisasi untuk Owner tak perlu ditandatangani karena hanya form persetujuan; yang membuktikan sahnya adalah `code` yang kembali.
pub fn url_otorisasi(cfg: &ShopeeConfig, state: &str) -> AppResult<String> {
    if cfg.auth_url.is_empty() || cfg.redirect_uri.is_empty() {
        return Err(AppError::bad_request(
            "SHOPEE_AUTH_URL dan SHOPEE_REDIRECT_URI belum diisi di konfigurasi server.",
        ));
    }

    Ok(format!(
        "{}?partner_id={}&auth_type=seller&response_type=code&redirect_uri={}&state={}",
        cfg.auth_url.trim_end_matches('/'),
        cfg.partner_id,
        client::escape(&cfg.redirect_uri),
        client::escape(state)
    ))
}

/// Menukar `code` dari callback menjadi access token + refresh token.
pub async fn tukar_kode_dengan_token(
    pool: &PgPool,
    cfg: &ShopeeConfig,
    kunci_enkripsi: &[u8; 32],
    code: &str,
    shop_id: i64,
) -> AppResult<DateTime<Utc>> {
    let baru = minta_token(
        cfg,
        PATH_TUKAR_KODE,
        serde_json::json!({
            "code": code,
            "shop_id": shop_id,
            "partner_id": cfg.partner_id,
        }),
    )
    .await?;

    token::simpan(
        pool,
        kunci_enkripsi,
        NAMA_PLATFORM,
        &shop_id.to_string(),
        &baru.access_token,
        &baru.refresh_token,
        baru.kedaluwarsa,
    )
    .await?;

    Ok(baru.kedaluwarsa)
}

/// Mengambil kredensial siap pakai, memperbaruinya dulu bila mendekati kedaluwarsa.
pub async fn token_yang_berlaku(
    pool: &PgPool,
    cfg: &ShopeeConfig,
    kunci_enkripsi: &[u8; 32],
) -> AppResult<Kredensial> {
    let tersimpan = token::muat(pool, kunci_enkripsi, NAMA_PLATFORM, NAMA_TAMPILAN).await?;
    let shop_id = parse_shop_id(&tersimpan.shop_ref)?;

    if !tersimpan.hampir_kedaluwarsa() {
        return Ok(Kredensial {
            shop_id,
            access_token: tersimpan.access_token,
        });
    }

    let refresh_token = tersimpan.refresh_token.ok_or_else(|| {
        AppError::bad_request("Token Shopee kedaluwarsa dan tidak ada refresh token.")
    })?;

    perbarui_token(pool, cfg, kunci_enkripsi, shop_id, &refresh_token).await
}

async fn perbarui_token(
    pool: &PgPool,
    cfg: &ShopeeConfig,
    kunci_enkripsi: &[u8; 32],
    shop_id: i64,
    refresh_token: &str,
) -> AppResult<Kredensial> {
    let baru = minta_token(
        cfg,
        PATH_PERBARUI,
        serde_json::json!({
            "refresh_token": refresh_token,
            "shop_id": shop_id,
            "partner_id": cfg.partner_id,
        }),
    )
    .await?;

    // Refresh token Shopee sekali pakai: setelah permintaan berhasil yang lama mati, jadi menyimpan hasilnya wajib.
    token::simpan(
        pool,
        kunci_enkripsi,
        NAMA_PLATFORM,
        &shop_id.to_string(),
        &baru.access_token,
        &baru.refresh_token,
        baru.kedaluwarsa,
    )
    .await?;

    Ok(Kredensial {
        shop_id,
        access_token: baru.access_token,
    })
}

/// Endpoint token publik: ditandatangani tanpa access token dan shop id karena belum ada keduanya.
async fn minta_token(
    cfg: &ShopeeConfig,
    path: &str,
    body: serde_json::Value,
) -> AppResult<TokenBaru> {
    let url = client::url_publik(cfg, path);

    let res = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "gagal menghubungi Shopee untuk token");
            AppError::bad_request("Tidak bisa menghubungi Shopee. Coba lagi sebentar lagi.")
        })?;

    let body: TokenResponse = res.json().await.map_err(|err| {
        tracing::error!(error = %err, "jawaban token Shopee tidak bisa dibaca");
        AppError::bad_request("Jawaban dari Shopee tidak dikenali.")
    })?;

    // Shopee menandai sukses dengan `error` kosong, bukan kode angka seperti TikTok.
    if !body.error.is_empty() {
        return Err(AppError::bad_request(format!(
            "Shopee menolak permintaan token: {} ({})",
            body.message, body.error
        )));
    }

    let (Some(access_token), Some(refresh_token), Some(expire_in)) =
        (body.access_token, body.refresh_token, body.expire_in)
    else {
        return Err(AppError::bad_request(
            "Shopee menjawab tanpa token. Coba hubungkan ulang tokonya.",
        ));
    };

    Ok(TokenBaru {
        access_token,
        refresh_token,
        kedaluwarsa: kedaluwarsa_dari(expire_in, Utc::now()),
    })
}

/// `expire_in` dibedakan lewat besarannya (dokumentasi Shopee tak konsisten: durasi 14400 vs epoch); di atas ambang berarti epoch, kalau salah token tak pernah diperbarui.
fn kedaluwarsa_dari(expire_in: i64, sekarang: DateTime<Utc>) -> DateTime<Utc> {
    const AMBANG_EPOCH: i64 = 1_000_000_000;

    if expire_in > AMBANG_EPOCH {
        DateTime::from_timestamp(expire_in, 0).unwrap_or(sekarang)
    } else {
        sekarang + Duration::seconds(expire_in)
    }
}

/// `shop_id_external` disimpan sebagai teks karena kolomnya dipakai platform lain yang identitas tokonya bukan angka.
fn parse_shop_id(raw: &str) -> AppResult<i64> {
    raw.trim().parse().map_err(|_| {
        tracing::error!(shop_ref = raw, "shop_id Shopee tersimpan bukan angka");
        AppError::bad_request("Data koneksi Shopee rusak. Hubungkan ulang tokonya.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ShopeeConfig {
        ShopeeConfig {
            partner_id: 1001141,
            partner_key: "rahasia".into(),
            host: "https://partner.test".into(),
            auth_url: "https://open.shopee.test/auth".into(),
            redirect_uri: "https://aj33.test/platforms/shopee/callback".into(),
        }
    }

    #[test]
    fn url_otorisasi_memuat_partner_dan_redirect() {
        let url = url_otorisasi(&cfg(), "acak-123").unwrap();

        assert!(url.starts_with("https://open.shopee.test/auth?"));
        assert!(url.contains("partner_id=1001141"));
        assert!(url.contains("auth_type=seller"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=acak-123"));
    }

    #[test]
    fn redirect_uri_di_escape() {
        // Tanpa di-escape, `:` dan `/` memotong query string dan Shopee mengembalikan Owner ke alamat yang salah.
        let url = url_otorisasi(&cfg(), "s").unwrap();
        assert!(
            url.contains("redirect_uri=https%3A%2F%2Faj33.test%2Fplatforms%2Fshopee%2Fcallback")
        );
    }

    #[test]
    fn url_otorisasi_menolak_konfigurasi_yang_belum_lengkap() {
        let mut c = cfg();
        c.redirect_uri = String::new();
        assert!(url_otorisasi(&c, "s").is_err());

        let mut c = cfg();
        c.auth_url = String::new();
        assert!(url_otorisasi(&c, "s").is_err());
    }

    #[test]
    fn expire_in_berupa_durasi_dihitung_dari_sekarang() {
        let sekarang = DateTime::parse_from_rfc3339("2026-09-16T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            kedaluwarsa_dari(14400, sekarang).to_rfc3339(),
            "2026-09-16T14:00:00+00:00"
        );
    }

    #[test]
    fn expire_in_berupa_epoch_dipakai_apa_adanya() {
        let sekarang = DateTime::parse_from_rfc3339("2026-09-16T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // 1767001812 = 2025-12-29T09:50:12Z.
        let hasil = kedaluwarsa_dari(1_767_001_812, sekarang);
        assert_eq!(hasil.timestamp(), 1_767_001_812);
    }

    #[test]
    fn shop_id_tersimpan_harus_angka() {
        assert_eq!(parse_shop_id("322300222").unwrap(), 322300222);
        assert_eq!(parse_shop_id(" 322300222 ").unwrap(), 322300222);
        assert!(parse_shop_id("shop_cipher_punya_tiktok").is_err());
    }
}
