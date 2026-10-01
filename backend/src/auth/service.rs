//! Aturan autentikasi (siapa boleh masuk, token dibuat dan dibaca); tanpa SQL, itu urusan `repo.rs`.

use super::repo::{self, UserRow};
use super::throttle::Throttle;
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

/// Bentuk user yang boleh dilihat frontend; `password_hash` tak pernah ikut walau sudah di-hash.
#[derive(Debug, Serialize)]
pub struct PublicUser {
    pub id: Uuid,
    pub name: String,
    pub email_or_username: String,
    pub role: Role,
    pub phone: Option<String>,
    pub is_active: bool,
    /// Selama true frontend menahan pengguna di halaman ganti password; ikut ke frontend (bukan token) agar pencabutan berlaku seketika.
    pub must_change_password: bool,
}

impl TryFrom<UserRow> for PublicUser {
    type Error = AppError;

    fn try_from(row: UserRow) -> Result<Self, Self::Error> {
        // Database sudah menjaga lewat CHECK, jadi nilai di luar daftar berarti ada yang menulis langsung ke tabel: bug, bukan salah pengguna.
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
            must_change_password: row.must_change_password,
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
    // Diperiksa sebelum menyentuh database: percobaan melewati jatah tak pantas membebani Postgres dan bcrypt yang sengaja lambat.
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

    // Akun tak ada, nonaktif, dan password salah memberi pesan sama persis agar login tak bisa dipakai menebak username terdaftar.
    let Some(user) = user.filter(|u| u.is_active) else {
        // Tetap verifikasi terhadap hash palsu agar waktu respons username tak ada mirip dengan yang ada.
        let _ = bcrypt::verify(password, HASH_UMPAN);
        // Username tak ada pun dihitung, kalau tidak perbedaan perilaku itu sendiri jadi cara menebak username nyata.
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
    // Terbukti tahu password sehingga percobaan gagal sebelumnya tak relevan; salah ketik beberapa kali lalu berhasil tak boleh menahan login berikutnya.
    throttle.bersihkan(email_or_username);
    Ok((token, public))
}

/// Hash bcrypt valid dari 32 byte acak yang dibuang agar login ke username tak terdaftar memakan waktu setara; hash malformed ditolak seketika dan merusak tujuannya.
const HASH_UMPAN: &str = "$2b$10$WW5CUrknix4wAipG9Qv9MO.gebjJWMbaLOyjIb8zVsIGt/VAVMNZ2";

pub async fn get_me(pool: &PgPool, user_id: Uuid) -> AppResult<PublicUser> {
    let user = repo::find_by_id(pool, user_id)
        .await?
        .filter(|u| u.is_active)
        .ok_or_else(|| AppError::unauthorized("Akun tidak ditemukan."))?;

    user.try_into()
}

/// Biaya bcrypt password buatan aplikasi, sama dengan hash di database agar waktu verifikasi tak bergantung akun.
const BIAYA_BCRYPT: u32 = 10;

/// Panjang minimum password; angka sama dijaga di frontend tapi yang menegakkan di sini.
const PANJANG_MINIMUM: usize = 8;

/// Ganti password sendiri tetap meminta password lama walau token sah, karena token bisa terbawa di perangkat yang ditinggal terbuka dan penemunya bisa mengunci pemilik keluar.
pub async fn change_password(
    pool: &PgPool,
    user_id: Uuid,
    password_lama: &str,
    password_baru: &str,
) -> AppResult<PublicUser> {
    let user = repo::find_by_id(pool, user_id)
        .await?
        .filter(|u| u.is_active)
        .ok_or_else(|| AppError::unauthorized("Akun tidak ditemukan."))?;

    let cocok = bcrypt::verify(password_lama, &user.password_hash).map_err(|err| {
        tracing::error!(user_id = %user.id, error = %err, "hash password tidak bisa diverifikasi");
        AppError::unauthorized("Password lama salah.")
    })?;

    if !cocok {
        return Err(AppError::unauthorized("Password lama salah."));
    }

    // Dihitung dalam karakter, bukan byte, agar "delapan huruf" bermakna sama untuk huruf beraksen.
    if password_baru.chars().count() < PANJANG_MINIMUM {
        return Err(AppError::bad_request(format!(
            "Password baru minimal {PANJANG_MINIMUM} karakter."
        )));
    }

    // bcrypt memotong masukan di 72 byte; tanpa penolakan, dua password panjang dengan 72 byte pertama sama sama-sama bisa masuk diam-diam.
    if password_baru.len() > 72 {
        return Err(AppError::bad_request(
            "Password baru terlalu panjang (maksimal 72 karakter).",
        ));
    }

    if password_baru == password_lama {
        return Err(AppError::bad_request(
            "Password baru harus berbeda dari password lama.",
        ));
    }

    let hash = bcrypt::hash(password_baru, BIAYA_BCRYPT).map_err(|err| {
        tracing::error!(user_id = %user.id, error = %err, "gagal membuat hash password");
        AppError::internal("Gagal menyimpan password baru.")
    })?;

    repo::update_password(pool, user.id, &hash).await?;

    // Dibaca ulang, bukan disusun dari `user` basi, karena yang dikembalikan harus memuat `must_change_password` yang sudah mati untuk melepas pengguna dari halaman ganti password.
    get_me(pool, user.id).await
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

/// Kebalikan `buat_token`; semua kegagalan (kedaluwarsa, tanda tangan salah, isi tak masuk akal) berpesan sama karena bagi pengguna sama-sama harus mengulang sesi.
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
        // Bila hash ini ditolak bcrypt sebagai malformed, jalur "username tak ada" lebih cepat dari "password salah" dan selisih waktunya membocorkan username terdaftar.
        assert!(matches!(bcrypt::verify("apa pun", HASH_UMPAN), Ok(false)));
    }
}
