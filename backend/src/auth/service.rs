//! Aturan main autentikasi: siapa boleh masuk, token dibuat dan dibaca
//! bagaimana. Tidak ada SQL di sini -- itu urusan `repo.rs`.

use super::throttle::Throttle;
use super::repo::{self, UserRow};
use super::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Sesi login berlaku 8 jam, sama seperti proyek lama.
const TOKEN_BERLAKU_JAM: i64 = 8;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: i64,
}

/// Bentuk user yang boleh dilihat frontend. `password_hash` tidak pernah
/// ikut, walaupun sudah di-hash.
#[derive(Debug, Serialize)]
pub struct PublicUser {
    pub id: Uuid,
    pub name: String,
    pub email_or_username: String,
    pub role: Role,
    pub phone: Option<String>,
    pub is_active: bool,
}

impl TryFrom<UserRow> for PublicUser {
    type Error = AppError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        // Database sudah menjaga lewat CHECK constraint, jadi nilai di luar
        // daftar berarti ada yang menulis langsung ke tabel -- itu bug, bukan
        // kesalahan pengguna.
        let role = Role::parse(&row.role).ok_or_else(|| {
            tracing::error!(user_id = %row.id, role = %row.role, "role tidak dikenal di database");
            AppError::unauthorized("Akun tidak valid.")
        })?;

        Ok(Self {
            id: row.id,
            name: row.name,
            email_or_username: row.email_or_username,
            role,
            phone: row.phone,
            is_active: row.is_active,
        })
    }
}

pub async fn login(
    pool: &PgPool,
    jwt_secret: &str,
    throttle: &Throttle,
    email_or_username: &str,
    password: &str,
) -> AppResult<(String, PublicUser)> {
    // Diperiksa sebelum menyentuh database sama sekali: percobaan yang sudah
    // melewati jatah tidak pantas membebani Postgres, apalagi bcrypt yang
    // memang sengaja lambat.
    if let Some(sisa) = throttle.sisa_tunggu(email_or_username) {
        let menit = (sisa.as_secs() / 60) + 1;
        tracing::warn!(
            username = %email_or_username,
            "percobaan login ditahan, jatah habis"
        );
        return Err(AppError::too_many_requests(format!(
            "Terlalu banyak percobaan masuk. Coba lagi dalam {menit} menit."
        )));
    }

    let user = repo::find_by_username(pool, email_or_username).await?;

    // Akun tidak ada, akun nonaktif, dan password salah sengaja memberi
    // pesan yang sama persis -- supaya halaman login tidak bisa dipakai
    // menebak username mana yang terdaftar.
    let Some(user) = user.filter(|u| u.is_active) else {
        // Tetap jalankan verifikasi terhadap hash palsu supaya waktu respons
        // untuk username yang tidak ada mirip dengan yang ada.
        let _ = bcrypt::verify(password, HASH_UMPAN);
        // Username yang tidak ada pun dihitung. Kalau hanya yang terdaftar
        // yang dibatasi, perbedaan perilakunya sendiri jadi cara menebak
        // username mana yang nyata.
        throttle.catat_gagal(email_or_username);
        return Err(AppError::unauthorized("Username atau password salah."));
    };

    let cocok = bcrypt::verify(password, &user.password_hash).map_err(|err| {
        tracing::error!(user_id = %user.id, error = %err, "hash password tidak bisa diverifikasi");
        AppError::unauthorized("Username atau password salah.")
    })?;

    if !cocok {
        throttle.catat_gagal(email_or_username);
        return Err(AppError::unauthorized("Username atau password salah."));
    }

    let public: PublicUser = user.try_into()?;
    let token = buat_token(jwt_secret, public.id, public.role)?;
    // Terbukti tahu passwordnya, jadi percobaan gagal sebelumnya tidak lagi
    // relevan -- salah ketik beberapa kali lalu berhasil tidak boleh
    // meninggalkan jejak yang menahan login berikutnya.
    throttle.bersihkan(email_or_username);
    Ok((token, public))
}

/// Hash bcrypt yang valid tapi tidak akan pernah cocok dengan password apa
/// pun. Dipakai agar percobaan login ke username yang tidak terdaftar tetap
/// memakan waktu komputasi yang setara.
/// Ini hash bcrypt sungguhan dari 32 byte acak yang langsung dibuang, jadi
/// tidak ada password yang bisa mencocokinya. Test di bawah menjaga agar
/// bentuknya tetap valid -- hash yang malformed ditolak bcrypt seketika dan
/// justru membuat jalur ini jauh lebih cepat, yang merusak tujuannya.
const HASH_UMPAN: &str = "$2b$10$WW5CUrknix4wAipG9Qv9MO.gebjJWMbaLOyjIb8zVsIGt/VAVMNZ2";

pub async fn get_me(pool: &PgPool, user_id: Uuid) -> AppResult<PublicUser> {
    let user = repo::find_by_id(pool, user_id)
        .await?
        .filter(|u| u.is_active)
        .ok_or_else(|| AppError::unauthorized("Akun tidak ditemukan."))?;

    user.try_into()
}

fn buat_token(secret: &str, user_id: Uuid, role: Role) -> AppResult<String> {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.as_str().to_string(),
        exp: (Utc::now() + Duration::hours(TOKEN_BERLAKU_JAM)).timestamp(),
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| {
        tracing::error!(error = %err, "gagal membuat token");
        AppError::unauthorized("Gagal membuat sesi login.")
    })
}

/// Kebalikan `buat_token`. Semua kegagalan -- kedaluwarsa, tanda tangan
/// salah, isi tidak masuk akal -- menghasilkan pesan yang sama, karena
/// bagi pengguna bedanya tidak ada: sesinya harus diulang.
pub fn baca_token(secret: &str, token: &str) -> AppResult<CurrentUser> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| AppError::unauthorized("Sesi login tidak valid atau sudah habis."))?;

    let id = Uuid::parse_str(&data.claims.sub)
        .map_err(|_| AppError::unauthorized("Sesi login tidak valid atau sudah habis."))?;

    let role = Role::parse(&data.claims.role)
        .ok_or_else(|| AppError::unauthorized("Sesi login tidak valid atau sudah habis."))?;

    Ok(CurrentUser { id, role })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "rahasia-untuk-test";

    #[test]
    fn token_yang_dibuat_bisa_dibaca_kembali() {
        let id = Uuid::new_v4();
        let token = buat_token(SECRET, id, Role::Kasir).unwrap();

        let user = baca_token(SECRET, &token).unwrap();
        assert_eq!(user.id, id);
        assert_eq!(user.role, Role::Kasir);
    }

    #[test]
    fn token_dengan_secret_berbeda_ditolak() {
        let token = buat_token(SECRET, Uuid::new_v4(), Role::Owner).unwrap();
        assert!(baca_token("secret-lain", &token).is_err());
    }

    #[test]
    fn token_kedaluwarsa_ditolak() {
        let claims = Claims {
            sub: Uuid::new_v4().to_string(),
            role: "owner".to_string(),
            exp: (Utc::now() - Duration::hours(1)).timestamp(),
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();

        assert!(baca_token(SECRET, &token).is_err());
    }

    #[test]
    fn hash_umpan_valid_tapi_tidak_pernah_cocok() {
        // Kalau hash ini ditolak bcrypt sebagai malformed, jalur "username
        // tidak ada" jadi jauh lebih cepat daripada jalur password salah,
        // dan perbedaan waktunya membocorkan username mana yang terdaftar.
        assert!(matches!(bcrypt::verify("apa pun", HASH_UMPAN), Ok(false)));
    }
}
