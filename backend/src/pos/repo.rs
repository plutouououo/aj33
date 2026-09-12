//! Akses tabel `transactions` dan `transaction_items`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct TransactionItem {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name_snapshot: String,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Transaction {
    pub id: Uuid,
    pub idempotency_key: String,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub customer_id: Option<Uuid>,
    pub cashier_user_id: Uuid,
    pub payment_method: String,
    pub subtotal: Decimal,
    pub total_amount: Decimal,
    pub amount_paid: Option<Decimal>,
    pub change_amount: Option<Decimal>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub items: Vec<TransactionItem>,
}

/// Baris `transactions` tanpa itemnya. Dipakai sebagai perantara sebelum
/// item-nya ikut dibaca.
struct TransactionHead {
    id: Uuid,
    idempotency_key: String,
    transaction_type: String,
    customer_id: Option<Uuid>,
    cashier_user_id: Uuid,
    payment_method: String,
    subtotal: Decimal,
    total_amount: Decimal,
    amount_paid: Option<Decimal>,
    change_amount: Option<Decimal>,
    status: String,
    created_at: DateTime<Utc>,
}

async fn lengkapi(pool: &PgPool, head: TransactionHead) -> AppResult<Transaction> {
    let items = sqlx::query_as!(
        TransactionItem,
        r#"
        SELECT id, product_id, product_name_snapshot, qty, unit_price, subtotal
        FROM transaction_items
        WHERE transaction_id = $1
        ORDER BY created_at, id
        "#,
        head.id
    )
    .fetch_all(pool)
    .await?;

    Ok(Transaction {
        id: head.id,
        idempotency_key: head.idempotency_key,
        transaction_type: head.transaction_type,
        customer_id: head.customer_id,
        cashier_user_id: head.cashier_user_id,
        payment_method: head.payment_method,
        subtotal: head.subtotal,
        total_amount: head.total_amount,
        amount_paid: head.amount_paid,
        change_amount: head.change_amount,
        status: head.status,
        created_at: head.created_at,
        items,
    })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> AppResult<Option<Transaction>> {
    let head = sqlx::query_as!(
        TransactionHead,
        r#"
        SELECT id, idempotency_key, type AS transaction_type, customer_id,
               cashier_user_id, payment_method, subtotal, total_amount,
               amount_paid, change_amount, status, created_at AS "created_at!"
        FROM transactions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    match head {
        Some(head) => Ok(Some(lengkapi(pool, head).await?)),
        None => Ok(None),
    }
}

pub async fn find_by_idempotency_key(pool: &PgPool, key: &str) -> AppResult<Option<Transaction>> {
    let head = sqlx::query_as!(
        TransactionHead,
        r#"
        SELECT id, idempotency_key, type AS transaction_type, customer_id,
               cashier_user_id, payment_method, subtotal, total_amount,
               amount_paid, change_amount, status, created_at AS "created_at!"
        FROM transactions
        WHERE idempotency_key = $1
        "#,
        key
    )
    .fetch_optional(pool)
    .await?;

    match head {
        Some(head) => Ok(Some(lengkapi(pool, head).await?)),
        None => Ok(None),
    }
}

/// Bahan mentah untuk menghitung ulang sidik jari transaksi lama.
pub async fn item_bahan_sidik_jari(
    pool: &PgPool,
    transaction_id: Uuid,
) -> AppResult<Vec<(Uuid, i32)>> {
    let rows = sqlx::query!(
        "SELECT product_id, qty FROM transaction_items WHERE transaction_id = $1",
        transaction_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| (r.product_id, r.qty)).collect())
}

pub struct NewTransactionItem {
    pub product_id: Uuid,
    pub product_name_snapshot: String,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

pub struct NewTransaction {
    pub idempotency_key: String,
    pub transaction_type: String,
    pub customer_id: Option<Uuid>,
    pub cashier_user_id: Uuid,
    pub payment_method: String,
    pub subtotal: Decimal,
    pub total_amount: Decimal,
    pub amount_paid: Option<Decimal>,
    pub change_amount: Option<Decimal>,
}

pub async fn insert_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    input: &NewTransaction,
    items: &[NewTransactionItem],
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO transactions
            (idempotency_key, type, customer_id, cashier_user_id, payment_method,
             subtotal, total_amount, amount_paid, change_amount)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
        input.idempotency_key,
        input.transaction_type,
        input.customer_id,
        input.cashier_user_id,
        input.payment_method,
        input.subtotal,
        input.total_amount,
        input.amount_paid,
        input.change_amount
    )
    .fetch_one(&mut **tx)
    .await?;

    for item in items {
        sqlx::query!(
            r#"
            INSERT INTO transaction_items
                (transaction_id, product_id, product_name_snapshot, qty, unit_price, subtotal)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            item.product_id,
            item.product_name_snapshot,
            item.qty,
            item.unit_price,
            item.subtotal
        )
        .execute(&mut **tx)
        .await?;
    }

    Ok(id)
}

pub async fn list_transactions(pool: &PgPool, limit: i64) -> AppResult<Vec<Transaction>> {
    let heads = sqlx::query_as!(
        TransactionHead,
        r#"
        SELECT id, idempotency_key, type AS transaction_type, customer_id,
               cashier_user_id, payment_method, subtotal, total_amount,
               amount_paid, change_amount, status, created_at AS "created_at!"
        FROM transactions
        ORDER BY created_at DESC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    let mut hasil = Vec::with_capacity(heads.len());
    for head in heads {
        hasil.push(lengkapi(pool, head).await?);
    }

    Ok(hasil)
}
