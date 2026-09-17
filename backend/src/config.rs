//! Konfigurasi dibaca sekali saat start dan gagal cepat kalau ada yang
//! kurang. Lebih baik proses menolak hidup dengan pesan jelas daripada mati
//! di tengah request pertama yang kebetulan menyentuh setelan yang kosong.

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    /// Asal yang boleh memanggil API ini. Diisi URL frontend Astro.
    pub cors_origins: Vec<String>,
    /// Kunci enkripsi token marketplace, 32 byte hasil decode hex.
    pub token_encryption_key: [u8; 32],
    pub tiktok: TiktokConfig,
    pub shopee: ShopeeConfig,
}

/// Kredensial TikTok Shop kosong selama toko belum dihubungkan, jadi
/// setelan ini opsional -- backend tetap boleh hidup tanpanya. Yang menolak
/// bekerja adalah adapter-nya saat dipanggil, dengan pesan yang menyebut
/// variabel mana yang perlu diisi.
#[derive(Debug, Clone, Default)]
pub struct TiktokConfig {
    pub app_key: String,
    pub app_secret: String,
    /// Host API, misalnya `https://open-api.tiktokglobalshop.com`.
    pub host: String,
    /// Host halaman otorisasi seller di Partner Center.
    pub auth_host: String,
    pub service_id: String,
    pub redirect_uri: String,
}

impl TiktokConfig {
    /// Dipakai `/api/platforms` untuk menandai platform yang kredensialnya
    /// belum diisi, sama seperti `isPlatformConfigured()` di proyek lama.
    pub fn is_configured(&self) -> bool {
        !self.app_key.is_empty() && !self.app_secret.is_empty() && !self.host.is_empty()
    }
}

/// Kredensial Shopee Open API v2. Sama seperti TikTok, boleh kosong sampai
/// tokonya benar-benar dihubungkan.
#[derive(Debug, Clone, Default)]
pub struct ShopeeConfig {
    /// Partner ID dari Shopee Open Platform. Ikut ditandatangani sebagai
    /// angka, jadi disimpan sebagai angka -- bukan string yang harus
    /// di-parse ulang tiap kali menyusun tanda tangan.
    pub partner_id: i64,
    pub partner_key: String,
    /// Host API tanpa path, misalnya `https://partner.shopeemobile.com`.
    /// Path lengkap (`/api/v2/...`) ditulis di pemanggil, karena persis
    /// path itulah yang ikut ditandatangani.
    pub host: String,
    /// Halaman otorisasi seller, misalnya `https://open.shopee.com/auth`.
    pub auth_url: String,
    /// Harus URL yang bisa dijangkau Shopee. Saat development pakai tunnel.
    pub redirect_uri: String,
}

impl ShopeeConfig {
    pub fn is_configured(&self) -> bool {
        self.partner_id != 0 && !self.partner_key.is_empty() && !self.host.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("environment variable {0} wajib diisi")]
    Missing(&'static str),
    #[error("environment variable {name} tidak valid: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = required("DATABASE_URL")?;
        let jwt_secret = required("JWT_SECRET")?;

        let port = match env::var("PORT") {
            Ok(raw) => raw.parse().map_err(|_| ConfigError::Invalid {
                name: "PORT",
                reason: format!("'{raw}' bukan nomor port yang valid"),
            })?,
            Err(_) => 3000,
        };

        let cors_origins = env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:4321".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let token_encryption_key = parse_encryption_key(&required("TOKEN_ENCRYPTION_KEY")?)?;

        Ok(Self {
            database_url,
            port,
            jwt_secret,
            cors_origins,
            token_encryption_key,
            tiktok: TiktokConfig {
                app_key: optional("TIKTOK_APP_KEY"),
                app_secret: optional("TIKTOK_APP_SECRET"),
                host: optional("TIKTOK_HOST"),
                auth_host: optional("TIKTOK_AUTH_HOST"),
                service_id: optional("TIKTOK_SERVICE_ID"),
                redirect_uri: optional("TIKTOK_REDIRECT_URI"),
            },
            shopee: ShopeeConfig {
                partner_id: parse_partner_id(&optional("SHOPEE_PARTNER_ID"))?,
                partner_key: optional("SHOPEE_PARTNER_KEY"),
                host: optional("SHOPEE_HOST"),
                auth_url: optional("SHOPEE_AUTH_URL"),
                redirect_uri: optional("SHOPEE_REDIRECT_URI"),
            },
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::Missing(name)),
    }
}

/// Partner ID kosong berarti Shopee memang belum dipakai, dan itu sah.
/// Yang tidak sah adalah partner id yang diisi tapi bukan angka: itu salah
/// ketik yang kalau dibiarkan baru ketahuan sebagai tanda tangan ditolak,
/// jauh dari sumber masalahnya.
fn parse_partner_id(raw: &str) -> Result<i64, ConfigError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(0);
    }

    raw.parse().map_err(|_| ConfigError::Invalid {
        name: "SHOPEE_PARTNER_ID",
        reason: format!("'{raw}' bukan angka"),
    })
}

fn optional(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

/// Kunci ditulis sebagai 64 karakter hex supaya aman disimpan di file `.env`
/// dan di secret CI, tanpa karakter yang perlu di-escape.
fn parse_encryption_key(raw: &str) -> Result<[u8; 32], ConfigError> {
    let bytes = hex::decode(raw.trim()).map_err(|_| ConfigError::Invalid {
        name: "TOKEN_ENCRYPTION_KEY",
        reason: "harus berupa hex".to_string(),
    })?;

    bytes.try_into().map_err(|v: Vec<u8>| ConfigError::Invalid {
        name: "TOKEN_ENCRYPTION_KEY",
        reason: format!("harus 32 byte (64 karakter hex), dapat {} byte", v.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kunci_enkripsi_harus_tepat_32_byte() {
        let valid = "0".repeat(64);
        assert!(parse_encryption_key(&valid).is_ok());

        let terlalu_pendek = "0".repeat(32);
        assert!(parse_encryption_key(&terlalu_pendek).is_err());

        assert!(parse_encryption_key("bukan hex").is_err());
    }

    #[test]
    fn partner_id_kosong_berarti_shopee_belum_dipakai() {
        assert_eq!(parse_partner_id("").unwrap(), 0);
        assert_eq!(parse_partner_id("   ").unwrap(), 0);
    }

    #[test]
    fn partner_id_bukan_angka_ditolak_saat_start() {
        assert!(parse_partner_id("1001141").is_ok());
        assert!(parse_partner_id("bukan-angka").is_err());
    }

    #[test]
    fn shopee_dianggap_belum_dikonfigurasi_kalau_ada_yang_kosong() {
        let mut cfg = ShopeeConfig {
            partner_id: 1001141,
            partner_key: "kunci".into(),
            host: "https://partner.test".into(),
            ..Default::default()
        };
        assert!(cfg.is_configured());

        cfg.partner_id = 0;
        assert!(!cfg.is_configured());
    }

    #[test]
    fn tiktok_dianggap_belum_dikonfigurasi_kalau_ada_yang_kosong() {
        let mut cfg = TiktokConfig {
            app_key: "key".into(),
            app_secret: "secret".into(),
            host: "https://example.test".into(),
            ..Default::default()
        };
        assert!(cfg.is_configured());

        cfg.app_secret = String::new();
        assert!(!cfg.is_configured());
    }
}
