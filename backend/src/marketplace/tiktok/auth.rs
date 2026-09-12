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
use crate::marketplace::crypto;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Token diperbarui kalau sisa masa berlakunya kurang dari ini. Menunggu
/// sampai benar-benar kedaluwarsa berarti request pertama setelah itu
/// gagal, padahal bisa dicegah.
const AMBANG_PERPANJANG: Duration = Duration::minutes(5);

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

    simpan_token(
        pool,
        kunci_enkripsi,
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
    let baris = sqlx::query!(
        r#"
        SELECT shop_id_external, access_token_encrypted, refresh_token_encrypted, token_expires_at
        FROM platforms
        WHERE platform_name = $1 AND is_connected
        "#,
        NAMA_PLATFORM
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        AppError::bad_request("Belum ada toko TikTok yang terhubung. Hubungkan dulu di Pengaturan.")
    })?;

    let (Some(shop_cipher), Some(access_terenkripsi), Some(kedaluwarsa)) = (
        baris.shop_id_external,
        baris.access_token_encrypted,
        baris.token_expires_at,
    ) else {
        return Err(AppError::bad_request(
            "Data koneksi TikTok tidak lengkap. Hubungkan ulang tokonya.",
        ));
    };

    if kedaluwarsa - Utc::now() > AMBANG_PERPANJANG {
        let access_token = buka(kunci_enkripsi, &access_terenkripsi)?;
        return Ok(Kredensial {
            shop_cipher,
            access_token,
        });
    }

    let refresh_token = baris
        .refresh_token_encrypted
        .ok_or_else(|| {
            AppError::bad_request("Token TikTok kedaluwarsa dan tidak ada refresh token.")
        })
        .and_then(|t| buka(kunci_enkripsi, &t))?;

    perbarui_token(pool, cfg, kunci_enkripsi, &shop_cipher, &refresh_token).await
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

    simpan_token(
        pool,
        kunci_enkripsi,
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

async fn simpan_token(
    pool: &PgPool,
    kunci: &[u8; 32],
    shop_cipher: &str,
    access_token: &str,
    refresh_token: &str,
    kedaluwarsa: DateTime<Utc>,
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO platforms
            (platform_name, shop_id_external, access_token_encrypted,
             refresh_token_encrypted, token_expires_at, is_connected, updated_at)
        VALUES ($1, $2, $3, $4, $5, true, now())
        ON CONFLICT (id) DO NOTHING
        RETURNING id
        "#,
        NAMA_PLATFORM,
        shop_cipher,
        crypto::encrypt(kunci, access_token),
        crypto::encrypt(kunci, refresh_token),
        kedaluwarsa
    )
    .fetch_optional(pool)
    .await?;

    if let Some(id) = id {
        return Ok(id);
    }

    // Baris platform biasanya sudah ada (dibuat seed atau koneksi
    // sebelumnya), jadi jalur yang lazim justru update.
    let id = sqlx::query_scalar!(
        r#"
        UPDATE platforms SET
            shop_id_external        = $2,
            access_token_encrypted  = $3,
            refresh_token_encrypted = $4,
            token_expires_at        = $5,
            is_connected            = true,
            updated_at              = now()
        WHERE platform_name = $1
        RETURNING id
        "#,
        NAMA_PLATFORM,
        shop_cipher,
        crypto::encrypt(kunci, access_token),
        crypto::encrypt(kunci, refresh_token),
        kedaluwarsa
    )
    .fetch_one(pool)
    .await?;

    Ok(id)
}

fn buka(kunci: &[u8; 32], terenkripsi: &str) -> AppResult<String> {
    crypto::decrypt(kunci, terenkripsi).map_err(|err| {
        // Ini hampir selalu berarti TOKEN_ENCRYPTION_KEY berubah sejak
        // token disimpan. Menghubungkan ulang toko akan menulis token baru
        // dengan kunci yang sekarang.
        tracing::error!(error = %err, "token marketplace tidak bisa didekripsi");
        AppError::bad_request(
            "Token toko tidak bisa dibuka. Hubungkan ulang toko TikTok di Pengaturan.",
        )
    })
}
