//! Akses tabel `push_subscriptions`.

use crate::error::AppResult;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct Langganan {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// Endpoint unik per perangkat; perangkat yang berganti akun pindah pemilik, kalau tidak notifikasi akun lama tetap sampai ke sana.
pub async fn simpan(pool: &PgPool, user_id: Uuid, langganan: &Langganan) -> AppResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (endpoint) DO UPDATE SET
            user_id = EXCLUDED.user_id,
            p256dh  = EXCLUDED.p256dh,
            auth    = EXCLUDED.auth
        "#,
        user_id,
        langganan.endpoint,
        langganan.p256dh,
        langganan.auth
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn milik(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<Langganan>> {
    let rows = sqlx::query_as!(
        Langganan,
        "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = $1",
        user_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// `user_id` kosong berarti siapa pun pemiliknya (owner membersihkan langganan yang ditolak layanan push).
pub async fn hapus(pool: &PgPool, endpoint: &str, user_id: Option<Uuid>) -> AppResult<()> {
    sqlx::query!(
        "DELETE FROM push_subscriptions WHERE endpoint = $1 AND ($2::uuid IS NULL OR user_id = $2)",
        endpoint,
        user_id
    )
    .execute(pool)
    .await?;

    Ok(())
}
