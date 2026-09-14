//! Akses tabel `customers`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Pelanggan toko.
///
/// `name` boleh kosong sejak skema awal: pelanggan yang lahir dari impor
/// pesanan marketplace kadang hanya membawa username, bukan nama. Yang
/// menampilkan harus siap menghadapinya.
#[derive(Debug, Serialize)]
pub struct Customer {
    pub id: Uuid,
    pub name: Option<String>,
    pub phone: Option<String>,
    /// `walk_in` untuk yang dibuat di kasir, `marketplace` untuk hasil impor.
    pub source: String,
    pub created_at: DateTime<Utc>,
}

pub struct CustomerFilter {
    pub search: Option<String>,
    pub limit: i64,
}

pub async fn list_customers(pool: &PgPool, filter: &CustomerFilter) -> AppResult<Vec<Customer>> {
    let rows = sqlx::query_as!(
        Customer,
        r#"
        SELECT id, name, phone, source, created_at AS "created_at!"
        FROM customers
        WHERE ($1::text IS NULL OR name ILIKE '%' || $1 || '%' OR phone ILIKE '%' || $1 || '%')
        ORDER BY name NULLS LAST, created_at DESC
        LIMIT $2
        "#,
        filter.search.as_deref(),
        filter.limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Pelanggan dengan nomor telepon tertentu, kalau ada.
///
/// Dipakai supaya pelanggan langganan yang kembali tidak melahirkan baris
/// baru setiap kali kasir mengetikkan namanya lagi. Memakai
/// `idx_customers_phone` yang sudah ada sejak skema awal.
pub async fn find_by_phone(pool: &PgPool, phone: &str) -> AppResult<Option<Customer>> {
    let row = sqlx::query_as!(
        Customer,
        r#"
        SELECT id, name, phone, source, created_at AS "created_at!"
        FROM customers
        WHERE phone = $1
        ORDER BY created_at
        LIMIT 1
        "#,
        phone
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// `source` diisi tegas, bukan mengandalkan default kolom: pelanggan yang
/// dibuat di meja kasir memang walk-in, dan menuliskannya berarti perubahan
/// default kolom di kemudian hari tidak bisa diam-diam menandai ulang mereka.
pub async fn insert_customer(
    pool: &PgPool,
    name: &str,
    phone: Option<&str>,
) -> AppResult<Customer> {
    let row = sqlx::query_as!(
        Customer,
        r#"
        INSERT INTO customers (name, phone, source)
        VALUES ($1, $2, 'walk_in')
        RETURNING id, name, phone, source, created_at AS "created_at!"
        "#,
        name,
        phone
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}
