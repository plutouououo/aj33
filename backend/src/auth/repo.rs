//! Akses tabel `users` dan `label_upload_tokens`, hanya membaca dan menulis baris tanpa aturan bisnis.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use serde::Serialize;
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

#[derive(Debug, Serialize)]
pub struct Pengguna {
    pub id: Uuid,
    pub name: String,
    pub role: String,
}

pub async fn list_aktif(pool: &PgPool, role: Option<&str>) -> AppResult<Vec<Pengguna>> {
    let rows = sqlx::query_as!(
        Pengguna,
        r#"
        SELECT id, name, role FROM users
        WHERE is_active AND ($1::text IS NULL OR role = $1)
        ORDER BY name
        "#,
        role
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[derive(Debug, Serialize)]
pub struct TokenLabel {
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

pub async fn token_label(pool: &PgPool, user_id: Uuid) -> AppResult<Option<TokenLabel>> {
    let row = sqlx::query_as!(
        TokenLabel,
        "SELECT created_at, last_used_at FROM label_upload_tokens WHERE user_id = $1",
        user_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn simpan_token_label(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
) -> AppResult<TokenLabel> {
    let row = sqlx::query_as!(
        TokenLabel,
        r#"
        INSERT INTO label_upload_tokens (user_id, token_hash)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET
            token_hash = EXCLUDED.token_hash,
            created_at = now(),
            last_used_at = NULL
        RETURNING created_at, last_used_at
        "#,
        user_id,
        token_hash
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn hapus_token_label(pool: &PgPool, user_id: Uuid) -> AppResult<()> {
    sqlx::query!(
        "DELETE FROM label_upload_tokens WHERE user_id = $1",
        user_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Pemilik token yang masih owner aktif, sekaligus mencatat waktu pakainya; akun yang dinonaktifkan atau berganti peran otomatis kehilangan aksesnya.
pub async fn pakai_token_label(pool: &PgPool, token_hash: &str) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar!(
        r#"
        UPDATE label_upload_tokens t
        SET last_used_at = now()
        FROM users u
        WHERE t.token_hash = $1 AND u.id = t.user_id AND u.is_active AND u.role = 'owner'
        RETURNING t.user_id
        "#,
        token_hash
    )
    .fetch_optional(pool)
    .await?;

    Ok(id)
}
