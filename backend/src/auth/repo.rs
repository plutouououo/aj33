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
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> AppResult<Option<UserRow>> {
    let row = sqlx::query_as!(
        UserRow,
        r#"
        SELECT id, name, email_or_username, password_hash, role, phone, is_active
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
        SELECT id, name, email_or_username, password_hash, role, phone, is_active
        FROM users
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}
