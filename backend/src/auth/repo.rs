//! Akses tabel `users`, hanya membaca dan menulis baris tanpa aturan bisnis.

use crate::error::AppResult;
use sqlx::PgPool;
use uuid::Uuid;

/// Cerminan baris `users`; `password_hash` ikut karena login butuh, makanya tipe ini tak pernah di-serialize (yang dikirim `PublicUser`).
#[derive(Debug)]
pub struct UserRow {
    pub id: Uuid,
    pub name: String,
    pub email_or_username: String,
    pub password_hash: String,
    pub role: String,
    pub phone: Option<String>,
    pub is_active: bool,
    /// Password dipasang orang lain (migrasi pemasangan akun) dan harus diganti pemiliknya sebelum akun dipakai.
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

/// Menyimpan password baru sekaligus mematikan penanda "harus ganti" dalam satu UPDATE, agar password terganti dengan penanda menyala tak mengunci pemiliknya.
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
