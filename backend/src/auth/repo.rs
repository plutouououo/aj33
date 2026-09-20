//! Akses tabel `users`. Tidak memuat aturan bisnis -- hanya membaca dan
//! menulis baris.

use crate::error::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

/// Cerminan baris `users` seperlunya modul ini. `password_hash` ikut karena
/// login membutuhkannya, dan justru karena itu tipe ini tidak pernah
/// di-serialize ke JSON -- `PublicUser` yang dikirim ke frontend.
#[derive(Debug)]
pub struct UserRow {
    pub id: Uuid,
    pub name: String,
    pub email_or_username: String,
    pub password_hash: String,
    pub role: String,
    pub phone: Option<String>,
    pub is_active: bool,
    /// Password yang dipasang orang lain (migrasi pemasangan akun) dan harus
    /// diganti pemiliknya sebelum akun ini dipakai bekerja.
    pub must_change_password: bool,
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> AppResult<Option<UserRow>> {
    let row = sqlx::query_as!(
        UserRow,
        r#"
        SELECT id, name, email_or_username, password_hash, role, phone, is_active,
               must_change_password
        FROM users
        WHERE email_or_username = $1
        "#,
        username
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> AppResult<Option<UserRow>> {
    let row = sqlx::query_as!(
        UserRow,
        r#"
        SELECT id, name, email_or_username, password_hash, role, phone, is_active,
               must_change_password
        FROM users
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Menyimpan password baru sekaligus mematikan penanda "harus ganti".
///
/// Keduanya dalam satu UPDATE, bukan dua: password yang sudah terganti tapi
/// penandanya masih menyala akan mengunci pemiliknya di halaman ganti
/// password selamanya.
pub async fn update_password(pool: &PgPool, id: Uuid, password_hash: &str) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $2,
            must_change_password = false,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        "#,
        id,
        password_hash
    )
    .execute(pool)
    .await?;

    Ok(())
}
