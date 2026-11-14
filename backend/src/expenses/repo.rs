//! Akses tabel `expenses`.

use crate::error::AppResult;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct Expense {
    pub id: Uuid,
    pub expense_date: NaiveDate,
    /// Kolom lama boleh kosong; laporan menampilkannya sebagai "Lainnya".
    pub category: Option<String>,
    pub amount: Decimal,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Isian beban yang sudah diperiksa.
pub struct BebanBaru {
    pub expense_date: NaiveDate,
    pub category: String,
    pub amount: Decimal,
    pub description: Option<String>,
}

pub struct SaringanBeban {
    pub dari: Option<NaiveDate>,
    pub sampai: Option<NaiveDate>,
    pub kategori: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// Satu halaman beban beserta jumlah baris dan total rupiah seluruh rentang, bukan hanya halaman ini.
pub async fn list(pool: &PgPool, s: &SaringanBeban) -> AppResult<(Vec<Expense>, i64, Decimal)> {
    let data = sqlx::query_as!(
        Expense,
        r#"
        SELECT e.id, e.expense_date, e.category, e.amount, e.description, e.created_at
        FROM expenses e
        WHERE ($1::date IS NULL OR e.expense_date >= $1)
          AND ($2::date IS NULL OR e.expense_date <= $2)
          AND ($3::text IS NULL OR COALESCE(NULLIF(trim(e.category), ''), 'Lainnya') = $3)
        ORDER BY e.expense_date DESC, e.created_at DESC, e.id
        LIMIT $4 OFFSET $5
        "#,
        s.dari,
        s.sampai,
        s.kategori.as_deref(),
        s.limit,
        s.offset
    )
    .fetch_all(pool)
    .await?;

    let jumlah = sqlx::query!(
        r#"
        SELECT count(*)                    AS "count!",
               COALESCE(SUM(e.amount), 0)  AS "total!"
        FROM expenses e
        WHERE ($1::date IS NULL OR e.expense_date >= $1)
          AND ($2::date IS NULL OR e.expense_date <= $2)
          AND ($3::text IS NULL OR COALESCE(NULLIF(trim(e.category), ''), 'Lainnya') = $3)
        "#,
        s.dari,
        s.sampai,
        s.kategori.as_deref()
    )
    .fetch_one(pool)
    .await?;

    Ok((data, jumlah.count, jumlah.total))
}

pub async fn find(pool: &PgPool, id: Uuid) -> AppResult<Option<Expense>> {
    let row = sqlx::query_as!(
        Expense,
        r#"
        SELECT e.id, e.expense_date, e.category, e.amount, e.description, e.created_at
        FROM expenses e
        WHERE e.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn insert(pool: &PgPool, b: &BebanBaru, oleh: Uuid) -> AppResult<Expense> {
    let row = sqlx::query_as!(
        Expense,
        r#"
        INSERT INTO expenses (expense_date, category, amount, description, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, expense_date, category, amount, description, created_at
        "#,
        b.expense_date,
        b.category,
        b.amount,
        b.description,
        oleh
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn update(pool: &PgPool, id: Uuid, b: &BebanBaru) -> AppResult<Option<Expense>> {
    let row = sqlx::query_as!(
        Expense,
        r#"
        UPDATE expenses
        SET expense_date = $2, category = $3, amount = $4, description = $5
        WHERE id = $1
        RETURNING id, expense_date, category, amount, description, created_at
        "#,
        id,
        b.expense_date,
        b.category,
        b.amount,
        b.description
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let hasil = sqlx::query!("DELETE FROM expenses WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(hasil.rows_affected() > 0)
}
