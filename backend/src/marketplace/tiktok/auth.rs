//! Otorisasi toko TikTok Shop dan penyimpanan tokennya.
//!
//! Alurnya: Owner membuka URL otorisasi Partner Center, mengizinkan aplikasi
//! mengakses tokonya, lalu TikTok mengarahkan balik ke `/callback` dengan
//! `auth_code`. Kode itu ditukar menjadi access token + refresh token, yang
//! disimpan terenkripsi di tabel `platforms`.

use super::client;
use super::NAMA_PLATFORM;
use crate::config::TiktokConfig;
use crate::error::{AppError, AppResult};
use crate::marketplace::token;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;

/// Nama platform untuk pesan yang dibaca Owner.
const NAMA_TAMPILAN: &str = "TikTok";

#[derive(Debug, Deserialize)]
struct TokenResponse {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<TokenData>,
}

#[derive(Debug, Deserialize)]
struct TokenData {
    access_token: String,
    access_token_expire_in: i64,
    refresh_token: String,
}

pub struct Kredensial {
    pub shop_cipher: String,
    pub access_token: String,
}

/// URL yang dibuka Owner untuk mengizinkan aplikasi mengakses tokonya.
pub fn url_otorisasi(cfg: &TiktokConfig, state: &str) -> AppResult<String> {
    if cfg.auth_host.is_empty() || cfg.service_id.is_empty() {
        return Err(AppError::bad_request(
            "TIKTOK_AUTH_HOST dan TIKTOK_SERVICE_ID belum diisi di konfigurasi server.",
        ));
    }

    Ok(format!(
        "{}?service_id={}&state={}",
        cfg.auth_host.trim_end_matches('/'),
        urlencoding(&cfg.service_id),
        urlencoding(state)
    ))
}

/// Peng-escape-an minimal untuk nilai query string. Cukup untuk dua nilai
/// yang dipakai di sini (service id dan state acak buatan kita sendiri).
fn urlencoding(nilai: &str) -> String {
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

pub async fn tukar_kode_dengan_token(
    pool: &PgPool,
    cfg: &TiktokConfig,
    kunci_enkripsi: &[u8; 32],
    auth_code: &str,
) -> AppResult<DateTime<Utc>> {
    let url = format!(
        "{}/api/v2/token/get?app_key={}&app_secret={}&auth_code={}&grant_type=authorized_code",
        cfg.host.trim_end_matches('/'),
        urlencoding(&cfg.app_key),
        urlencoding(&cfg.app_secret),
        urlencoding(auth_code)
    );

    let data = ambil_token(&url).await?;

    // Toko mana yang baru saja memberi izin hanya diketahui setelah token
    // di tangan, jadi urutannya memang begini: token dulu, baru tokonya.
    let shop_cipher = client::toko_pertama_yang_diizinkan(cfg, &data.access_token).await?;

    let kedaluwarsa = Utc::now() + Duration::seconds(data.access_token_expire_in);

    token::simpan(
        pool,
        kunci_enkripsi,
        NAMA_PLATFORM,
        &shop_cipher,
        &data.access_token,
        &data.refresh_token,
        kedaluwarsa,
    )
    .await?;

    Ok(kedaluwarsa)
}

/// Mengambil token yang siap pakai, memperbaruinya lebih dulu kalau sudah
/// mendekati kedaluwarsa.
pub async fn token_yang_berlaku(
    pool: &PgPool,
    cfg: &TiktokConfig,
    kunci_enkripsi: &[u8; 32],
) -> AppResult<Kredensial> {
    let tersimpan = token::muat(pool, kunci_enkripsi, NAMA_PLATFORM, NAMA_TAMPILAN).await?;

    if !tersimpan.hampir_kedaluwarsa() {
        return Ok(Kredensial {
            shop_cipher: tersimpan.shop_ref,
            access_token: tersimpan.access_token,
        });
    }

    let refresh_token = tersimpan.refresh_token.ok_or_else(|| {
        AppError::bad_request("Token TikTok kedaluwarsa dan tidak ada refresh token.")
    })?;

    perbarui_token(
        pool,
        cfg,
        kunci_enkripsi,
        &tersimpan.shop_ref.clone(),
        &refresh_token,
    )
    .await
}

async fn perbarui_token(
    pool: &PgPool,
    cfg: &TiktokConfig,
    kunci_enkripsi: &[u8; 32],
    shop_cipher: &str,
    refresh_token: &str,
) -> AppResult<Kredensial> {
    let url = format!(
        "{}/api/v2/token/refresh?app_key={}&app_secret={}&refresh_token={}&grant_type=refresh_token",
        cfg.host.trim_end_matches('/'),
        urlencoding(&cfg.app_key),
        urlencoding(&cfg.app_secret),
        urlencoding(refresh_token)
    );

    let data = ambil_token(&url).await?;
    let kedaluwarsa = Utc::now() + Duration::seconds(data.access_token_expire_in);

    token::simpan(
        pool,
        kunci_enkripsi,
        NAMA_PLATFORM,
        shop_cipher,
        &data.access_token,
        &data.refresh_token,
        kedaluwarsa,
    )
    .await?;

    Ok(Kredensial {
        shop_cipher: shop_cipher.to_string(),
        access_token: data.access_token,
    })
}

async fn ambil_token(url: &str) -> AppResult<TokenData> {
    let res = reqwest::get(url).await.map_err(|err| {
        tracing::error!(error = %err, "gagal menghubungi TikTok untuk token");
        AppError::bad_request("Tidak bisa menghubungi TikTok Shop. Coba lagi sebentar lagi.")
    })?;

    let body: TokenResponse = res.json().await.map_err(|err| {
        tracing::error!(error = %err, "jawaban token TikTok tidak bisa dibaca");
        AppError::bad_request("Jawaban dari TikTok Shop tidak dikenali.")
    })?;

    body.data.ok_or_else(|| {
        // Pesan dari TikTok ikut ditampilkan karena inilah satu-satunya
        // petunjuk kenapa otorisasi ditolak (kode kedaluwarsa, app key
        // salah, dan seterusnya).
        AppError::bad_request(format!(
            "TikTok Shop menolak permintaan token: {} ({})",
            body.message, body.code
        ))
    })
}
