//! Koneksi database.
//!
//! PENTING: RLS menyala di 30 tabel tapi tanpa satu pun policy (lihat
//! `db/migrations/0002_rls_and_checks.sql`), jadi hanya role pemilik tabel
//! yang bisa membaca. Koneksi harus memakai role yang sama dengan proyek
//! lama; mengganti role di `DATABASE_URL` akan membuat semua query
//! mengembalikan nol baris tanpa pesan error.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}
