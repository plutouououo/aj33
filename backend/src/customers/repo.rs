//! Akses tabel `customers`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Pelanggan beserta ringkasan belanja (`name` boleh kosong karena impor marketplace); omzet = `subtotal - discount_amount`, bukan `total_amount`, seperti laporan.
#[derive(Debug, Serialize)]
pub struct Customer {
    pub id: Uuid,
    pub name: Option<String>,
    pub phone: Option<String>,
    /// Alamat antar. `null` untuk pembeli yang datang ke toko.
    pub address: Option<String>,
    /// `walk_in` untuk yang dibuat di kasir, `marketplace` untuk hasil impor.
    pub source: String,
    pub created_at: DateTime<Utc>,
    /// Transaksi selesai atas nama pelanggan ini.
    pub purchase_count: i64,
    pub total_spent: Decimal,
    /// `null` kalau belum pernah belanja.
    pub last_purchase_at: Option<DateTime<Utc>>,
}

pub struct CustomerFilter {
    pub search: Option<String>,
    /// `belanja` = hanya yang pernah bertransaksi, `walk_in`/`marketplace` = asal pelanggan, kosong = semua.
    pub scope: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// Urutan daftar: nama untuk mencari orang, belanja untuk melihat siapa paling berharga.
pub enum Urutan {
    Nama,
    Belanja,
    Terbaru,
}

impl Urutan {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("belanja") => Self::Belanja,
            Some("terbaru") => Self::Terbaru,
            _ => Self::Nama,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Nama => "nama",
            Self::Belanja => "belanja",
            Self::Terbaru => "terbaru",
        }
    }
}

/// Daftar pelanggan beserta total baris yang cocok, agar halaman bisa mengatakan "menampilkan 25 dari 300", bukan diam-diam memotong di `limit`.
pub async fn list_customers(
    pool: &PgPool,
    filter: &CustomerFilter,
    urutan: &Urutan,
) -> AppResult<(Vec<Customer>, i64)> {
    let rows = sqlx::query_as!(
        Customer,
        r#"
        SELECT c.id, c.name, c.phone, c.address, c.source,
               c.created_at                       AS "created_at!",
               COALESCE(b.jumlah, 0)              AS "purchase_count!",
               COALESCE(b.total, 0)               AS "total_spent!",
               b.terakhir                         AS last_purchase_at
        FROM customers c
        LEFT JOIN (
            SELECT t.customer_id,
                   count(*)          AS jumlah,
                   SUM(t.subtotal - t.discount_amount) AS total,
                   max(t.created_at) AS terakhir
            FROM transactions t
            WHERE t.status = 'completed' AND t.customer_id IS NOT NULL
            GROUP BY t.customer_id
        ) b ON b.customer_id = c.id
        WHERE ($1::text IS NULL OR c.name ILIKE '%' || $1 || '%' OR c.phone ILIKE '%' || $1 || '%')
          AND ($2::text IS NULL
               OR ($2 = 'belanja' AND b.customer_id IS NOT NULL)
               OR c.source = $2)
        ORDER BY
            CASE WHEN $3 = 'belanja' THEN COALESCE(b.total, 0) END DESC NULLS LAST,
            CASE WHEN $3 = 'terbaru' THEN c.created_at END DESC NULLS LAST,
            CASE WHEN $3 = 'nama' THEN c.name END ASC NULLS LAST,
            c.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
        filter.search.as_deref(),
        filter.scope.as_deref(),
        urutan.as_str(),
        filter.limit,
        filter.offset
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM customers c
        WHERE ($1::text IS NULL OR c.name ILIKE '%' || $1 || '%' OR c.phone ILIKE '%' || $1 || '%')
          AND ($2::text IS NULL
               OR ($2 = 'belanja' AND EXISTS (
                   SELECT 1 FROM transactions t
                   WHERE t.customer_id = c.id AND t.status = 'completed'))
               OR c.source = $2)
        "#,
        filter.search.as_deref(),
        filter.scope.as_deref()
    )
    .fetch_one(pool)
    .await?;

    Ok((rows, total))
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> AppResult<Option<Customer>> {
    let row = sqlx::query_as!(
        Customer,
        r#"
        SELECT c.id, c.name, c.phone, c.address, c.source,
               c.created_at                                          AS "created_at!",
               (SELECT count(*) FROM transactions t
                WHERE t.customer_id = c.id AND t.status = 'completed') AS "purchase_count!",
               (SELECT COALESCE(SUM(t.subtotal - t.discount_amount), 0) FROM transactions t
                WHERE t.customer_id = c.id AND t.status = 'completed') AS "total_spent!",
               (SELECT max(t.created_at) FROM transactions t
                WHERE t.customer_id = c.id AND t.status = 'completed') AS last_purchase_at
        FROM customers c
        WHERE c.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Pelanggan dengan nomor telepon tertentu agar langganan yang kembali tak melahirkan baris baru tiap diketik ulang; memakai `idx_customers_phone` sejak skema awal.
pub async fn find_by_phone(pool: &PgPool, phone: &str) -> AppResult<Option<Customer>> {
    let id = sqlx::query_scalar!(
        r#"
        SELECT id FROM customers
        WHERE phone = $1
        ORDER BY created_at
        LIMIT 1
        "#,
        phone
    )
    .fetch_optional(pool)
    .await?;

    match id {
        Some(id) => find_by_id(pool, id).await,
        None => Ok(None),
    }
}

/// `source` diisi tegas, bukan default kolom, agar perubahan default kelak tak diam-diam menandai ulang pelanggan walk-in.
pub async fn insert_customer(
    pool: &PgPool,
    name: &str,
    phone: Option<&str>,
    address: Option<&str>,
) -> AppResult<Customer> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO customers (name, phone, address, source)
        VALUES ($1, $2, $3, 'walk_in')
        RETURNING id
        "#,
        name,
        phone,
        address
    )
    .fetch_one(pool)
    .await?;

    // Dibaca ulang lewat jalur sama dengan bacaan lain agar bentuk yang dikembalikan tak beda dari daftar.
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound.into())
}

/// Bidang yang boleh diubah: `None` = jangan sentuh, `Some(None)` = kosongkan; dibedakan di tipe agar tak ada handler yang menghapus telepon hanya karena form tak mengirimnya.
#[derive(Debug, Default)]
pub struct CustomerPatch {
    pub name: Option<String>,
    pub phone: Option<Option<String>>,
    pub address: Option<Option<String>>,
}

pub async fn update_customer(
    pool: &PgPool,
    id: Uuid,
    patch: &CustomerPatch,
) -> AppResult<Option<Customer>> {
    let terubah = sqlx::query_scalar!(
        r#"
        UPDATE customers
        SET name       = COALESCE($2, name),
            phone      = CASE WHEN $3::bool THEN $4 ELSE phone END,
            address    = CASE WHEN $5::bool THEN $6 ELSE address END,
            updated_at = now()
        WHERE id = $1
        RETURNING id
        "#,
        id,
        patch.name.as_deref(),
        patch.phone.is_some(),
        patch.phone.as_ref().and_then(|v| v.as_deref()),
        patch.address.is_some(),
        patch.address.as_ref().and_then(|v| v.as_deref()),
    )
    .fetch_optional(pool)
    .await?;

    match terubah {
        Some(id) => find_by_id(pool, id).await,
        None => Ok(None),
    }
}

/// Banyaknya rujukan yang menahan satu pelanggan dari penghapusan.
pub struct Rujukan {
    pub transactions: i64,
    pub orders: i64,
}

impl Rujukan {
    pub fn ada(&self) -> bool {
        self.transactions > 0 || self.orders > 0
    }
}

/// Foreign key skema awal `ON DELETE NO ACTION` sudah menolak hapus pelanggan yang dirujuk; dihitung di sini agar penolakan bisa dijelaskan ("masih punya 3 transaksi") alih-alih galat constraint.
pub async fn hitung_rujukan(pool: &PgPool, id: Uuid) -> AppResult<Rujukan> {
    let row = sqlx::query!(
        r#"
        SELECT (SELECT count(*) FROM transactions t WHERE t.customer_id = $1)
                   AS "transactions!",
               (SELECT count(*) FROM external_orders o WHERE o.customer_id = $1)
                   AS "orders!"
        "#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(Rujukan {
        transactions: row.transactions,
        orders: row.orders,
    })
}

pub async fn delete_customer(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let hasil = sqlx::query!("DELETE FROM customers WHERE id = $1", id)
        .execute(pool)
        .await?;

    Ok(hasil.rows_affected() > 0)
}

/// Satu produk pada daftar "yang sering dibeli".
#[derive(Debug, Serialize)]
pub struct FavoriteProduct {
    pub product_id: Option<Uuid>,
    /// Nama saat dibeli, bukan nama sekarang.
    pub name: String,
    pub qty: i64,
    pub spent: Decimal,
}

pub async fn favorite_products(
    pool: &PgPool,
    customer_id: Uuid,
    limit: i64,
) -> AppResult<Vec<FavoriteProduct>> {
    let rows = sqlx::query!(
        r#"
        SELECT ti.product_id                 AS product_id,
               max(ti.product_name_snapshot) AS "name!",
               SUM(ti.qty)::bigint           AS "qty!",
               SUM(ti.subtotal)              AS "spent!"
        FROM transaction_items ti
        JOIN transactions t ON t.id = ti.transaction_id
        WHERE t.customer_id = $1 AND t.status = 'completed'
        GROUP BY ti.product_id, CASE WHEN ti.product_id IS NULL THEN ti.product_name_snapshot END
        ORDER BY "spent!" DESC, "qty!" DESC
        LIMIT $2
        "#,
        customer_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| FavoriteProduct {
            product_id: r.product_id,
            name: r.name,
            qty: r.qty,
            spent: r.spent,
        })
        .collect())
}

/// Satu baris riwayat belanja.
#[derive(Debug, Serialize)]
pub struct PurchaseRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub payment_method: String,
    pub item_count: i64,
    pub revenue: Decimal,
    pub shipping: Decimal,
    pub total_amount: Decimal,
}

pub async fn purchases(
    pool: &PgPool,
    customer_id: Uuid,
    limit: i64,
) -> AppResult<Vec<PurchaseRow>> {
    let rows = sqlx::query_as!(
        PurchaseRow,
        r#"
        SELECT t.id                 AS "id!",
               t.created_at         AS "created_at!",
               t.payment_method     AS "payment_method!",
               t.subtotal - t.discount_amount AS "revenue!",
               t.shipping_charged   AS "shipping!",
               t.total_amount       AS "total_amount!",
               (SELECT count(*) FROM transaction_items ti WHERE ti.transaction_id = t.id)
                                    AS "item_count!"
        FROM transactions t
        WHERE t.customer_id = $1 AND t.status = 'completed'
        ORDER BY t.created_at DESC
        LIMIT $2
        "#,
        customer_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
