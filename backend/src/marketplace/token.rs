//! Penyimpanan token marketplace di tabel `platforms`.
//!
//! Dipakai bersama semua adapter. Bentuk penyimpanannya memang sama untuk
//! tiap platform -- satu baris per platform, token terenkripsi dengan
//! `crypto`, plus satu identitas toko di `shop_id_external` -- jadi logika
//! ini tinggal di satu tempat. Yang berbeda antar platform hanyalah ARTI
//! `shop_ref`: TikTok mengisinya dengan `shop_cipher`, Shopee dengan
//! `shop_id` berupa angka.
//!
//! Alasan ini bukan sekadar menghemat baris: enkripsi token adalah batas
//! keamanan. Kalau tiap adapter menyalin sendiri jalur simpan/bukanya, cukup
//! satu salinan lupa mengenkripsi untuk menaruh token mentah di basis data,
//! dan tidak ada satu tempat pun yang bisa diperiksa untuk memastikan itu
//! tidak terjadi.

use super::crypto;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Token diperbarui kalau sisa masa berlakunya kurang dari ini. Menunggu
/// sampai benar-benar kedaluwarsa berarti request pertama setelah itu
/// gagal, padahal bisa dicegah.
pub const AMBANG_PERPANJANG: Duration = Duration::minutes(5);

/// Token satu toko, sudah didekripsi.
#[derive(Debug)]
pub struct TokenTersimpan {
    /// Identitas toko di platform. Artinya ditentukan adapter.
    pub shop_ref: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub kedaluwarsa: DateTime<Utc>,
}

impl TokenTersimpan {
    pub fn hampir_kedaluwarsa(&self) -> bool {
        self.kedaluwarsa - Utc::now() <= AMBANG_PERPANJANG
    }
}

/// Membaca token platform yang sedang terhubung.
///
/// `nama_tampilan` hanya dipakai untuk pesan yang dibaca Owner ("Shopee",
/// "TikTok"), sementara `platform` adalah nilai di kolom `platform_name`.
pub async fn muat(
    pool: &PgPool,
    kunci: &[u8; 32],
    platform: &str,
    nama_tampilan: &str,
) -> AppResult<TokenTersimpan> {
    let baris = sqlx::query!(
        r#"
        SELECT shop_id_external, access_token_encrypted, refresh_token_encrypted, token_expires_at
        FROM platforms
        WHERE platform_name = $1 AND is_connected
        "#,
        platform
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        AppError::bad_request(format!(
            "Belum ada toko {nama_tampilan} yang terhubung. Hubungkan dulu di Pengaturan."
        ))
    })?;

    let (Some(shop_ref), Some(access_terenkripsi), Some(kedaluwarsa)) = (
        baris.shop_id_external,
        baris.access_token_encrypted,
        baris.token_expires_at,
    ) else {
        return Err(AppError::bad_request(format!(
            "Data koneksi {nama_tampilan} tidak lengkap. Hubungkan ulang tokonya."
        )));
    };

    let refresh_token = baris
        .refresh_token_encrypted
        .map(|t| buka(kunci, &t, nama_tampilan))
        .transpose()?;

    Ok(TokenTersimpan {
        shop_ref,
        access_token: buka(kunci, &access_terenkripsi, nama_tampilan)?,
        refresh_token,
        kedaluwarsa,
    })
}

/// Menyimpan token hasil otorisasi atau perpanjangan.
///
/// Satu pernyataan, bukan insert-lalu-update. Versi sebelumnya memakai
/// `ON CONFLICT (id) DO NOTHING` tanpa menyebut `id`, sehingga
/// `gen_random_uuid()` selalu memberi id yang belum ada: konflik yang
/// ditunggu tidak pernah terjadi, dan tiap penyimpanan token menambah baris
/// `platforms` baru alih-alih memperbarui yang lama. Kuncinya memang
/// `platform_name` -- satu baris per platform -- dan sejak migrasi 0010
/// database yang menegakkannya.
pub async fn simpan(
    pool: &PgPool,
    kunci: &[u8; 32],
    platform: &str,
    shop_ref: &str,
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
        ON CONFLICT (platform_name) DO UPDATE SET
            shop_id_external        = EXCLUDED.shop_id_external,
            access_token_encrypted  = EXCLUDED.access_token_encrypted,
            refresh_token_encrypted = EXCLUDED.refresh_token_encrypted,
            token_expires_at        = EXCLUDED.token_expires_at,
            is_connected            = true,
            updated_at              = now()
        RETURNING id
        "#,
        platform,
        shop_ref,
        crypto::encrypt(kunci, access_token),
        crypto::encrypt(kunci, refresh_token),
        kedaluwarsa
    )
    .fetch_one(pool)
    .await?;

    Ok(id)
}

fn buka(kunci: &[u8; 32], terenkripsi: &str, nama_tampilan: &str) -> AppResult<String> {
    crypto::decrypt(kunci, terenkripsi).map_err(|err| {
        // Ini hampir selalu berarti TOKEN_ENCRYPTION_KEY berubah sejak
        // token disimpan. Menghubungkan ulang toko akan menulis token baru
        // dengan kunci yang sekarang.
        tracing::error!(error = %err, platform = nama_tampilan, "token marketplace tidak bisa didekripsi");
        AppError::bad_request(format!(
            "Token toko tidak bisa dibuka. Hubungkan ulang toko {nama_tampilan} di Pengaturan."
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(kedaluwarsa: DateTime<Utc>) -> TokenTersimpan {
        TokenTersimpan {
            shop_ref: "toko".into(),
            access_token: "akses".into(),
            refresh_token: Some("perbarui".into()),
            kedaluwarsa,
        }
    }

    #[test]
    fn token_yang_masih_lama_belum_perlu_diperbarui() {
        assert!(!token(Utc::now() + Duration::hours(3)).hampir_kedaluwarsa());
    }

    #[test]
    fn token_yang_mendekati_tenggat_diperbarui_sebelum_gagal() {
        // Inti ambang ini: token yang tinggal semenit lagi masih "berlaku",
        // tapi request berikutnya bisa jatuh setelah tenggat.
        assert!(token(Utc::now() + Duration::minutes(1)).hampir_kedaluwarsa());
    }

    #[test]
    fn token_yang_sudah_lewat_dianggap_perlu_diperbarui() {
        assert!(token(Utc::now() - Duration::hours(1)).hampir_kedaluwarsa());
    }
}
