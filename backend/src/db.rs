//! Koneksi database; RLS menyala di 30 tabel tanpa policy (migrasi 0002), jadi hanya role pemilik tabel yang bisa membaca; ganti role di `DATABASE_URL` membuat query kosong tanpa error.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}
