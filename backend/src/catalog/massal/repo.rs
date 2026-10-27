//! SQL edit massal: sesi pratinjau dan pembacaan seluruh batch.

use crate::catalog::repo::ProductBatch;
use crate::error::AppResult;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct Sesi {
    pub id: Uuid,
    pub kelompok: String,
    pub file_name: String,
    pub status: String,
    pub baris: serde_json::Value,
    pub hasil: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub applied_at: Option<DateTime<Utc>>,
}

pub async fn batch_semua(pool: &PgPool) -> AppResult<Vec<ProductBatch>> {
    let rows = sqlx::query_as!(
        ProductBatch,
        r#"
        SELECT id, product_id, batch_number, purchase_price, quantity, remaining_qty,
               expiry_date, storage_location, received_at AS "received_at!", created_by
        FROM product_batches
        ORDER BY product_id, expiry_date NULLS LAST, received_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn simpan_sesi(
    pool: &PgPool,
    kelompok: &str,
    file_name: &str,
    baris: &serde_json::Value,
    created_by: Uuid,
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO edit_massal (kelompok, file_name, baris, created_by)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        kelompok,
        file_name,
        baris,
        created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn ambil_sesi(pool: &PgPool, id: Uuid) -> AppResult<Option<Sesi>> {
    let sesi = sqlx::query_as!(
        Sesi,
        r#"
        SELECT id, kelompok, file_name, status, baris, hasil,
               created_at AS "created_at!", applied_at
        FROM edit_massal
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(sesi)
}

/// Menandai sesi diterapkan dan mengembalikan barisnya; `None` bila sesi tak ada atau sudah diterapkan, sehingga dua klik Terapkan tak menjalankan perubahan dua kali.
pub async fn klaim_sesi(pool: &PgPool, id: Uuid) -> AppResult<Option<(String, serde_json::Value)>> {
    let row = sqlx::query!(
        r#"
        UPDATE edit_massal
        SET status = 'diterapkan', applied_at = now()
        WHERE id = $1 AND status = 'pratinjau'
        RETURNING kelompok, baris
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| (r.kelompok, r.baris)))
}

pub async fn simpan_hasil(pool: &PgPool, id: Uuid, hasil: &serde_json::Value) -> AppResult<()> {
    sqlx::query!("UPDATE edit_massal SET hasil = $2 WHERE id = $1", id, hasil)
        .execute(pool)
        .await?;

    Ok(())
}
