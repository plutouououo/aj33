//! Akses tabel `products` dan `categories`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Bentuk produk yang dikirim ke frontend. `category_name` hasil JOIN,
/// mengikuti skema `Product` di `contracts/api.yaml`.
#[derive(Debug, Serialize)]
pub struct Product {
    pub id: Uuid,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub name: String,
    pub sku: Option<String>,
    pub price: Decimal,
    pub cost_price: Option<Decimal>,
    pub stock_qty: i32,
    pub low_stock_threshold: i32,
    pub image_url: Option<String>,
    pub unit: Option<String>,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct StockAdjustment {
    pub id: Uuid,
    pub product_id: Uuid,
    pub change_qty: i32,
    pub reason: String,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub stock_before: i32,
    pub stock_after: i32,
    pub adjusted_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct ProductFilter {
    pub search: Option<String>,
    pub category_id: Option<Uuid>,
    pub only_active: bool,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_products(
    pool: &PgPool,
    filter: &ProductFilter,
) -> AppResult<(Vec<Product>, i64)> {
    // `$1 IS NULL OR ...` membuat satu query melayani semua kombinasi filter.
    // Alternatifnya merangkai SQL sebagai string, yang menutup pintu bagi
    // pemeriksaan query saat compile.
    let rows = sqlx::query_as!(
        Product,
        r#"
        SELECT
            p.id, p.category_id, c.name AS "category_name?", p.name, p.sku,
            p.price, p.cost_price, p.stock_qty, p.low_stock_threshold,
            p.image_url, p.unit, p.is_active, p.created_by,
            p.created_at AS "created_at!", p.updated_at AS "updated_at!"
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%' OR p.sku ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR p.category_id = $2)
          AND (NOT $3::bool OR p.is_active)
        ORDER BY p.name
        LIMIT $4 OFFSET $5
        "#,
        filter.search.as_deref(),
        filter.category_id,
        filter.only_active,
        filter.limit,
        filter.offset
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM products p
        WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%' OR p.sku ILIKE '%' || $1 || '%')
          AND ($2::uuid IS NULL OR p.category_id = $2)
          AND (NOT $3::bool OR p.is_active)
        "#,
        filter.search.as_deref(),
        filter.category_id,
        filter.only_active
    )
    .fetch_one(pool)
    .await?;

    Ok((rows, total))
}

pub async fn find_product(pool: &PgPool, id: Uuid) -> AppResult<Option<Product>> {
    let row = sqlx::query_as!(
        Product,
        r#"
        SELECT
            p.id, p.category_id, c.name AS "category_name?", p.name, p.sku,
            p.price, p.cost_price, p.stock_qty, p.low_stock_threshold,
            p.image_url, p.unit, p.is_active, p.created_by,
            p.created_at AS "created_at!", p.updated_at AS "updated_at!"
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE p.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub struct NewProduct {
    pub name: String,
    pub sku: Option<String>,
    pub category_id: Option<Uuid>,
    pub price: Decimal,
    pub cost_price: Option<Decimal>,
    pub stock_qty: i32,
    pub low_stock_threshold: i32,
    pub image_url: Option<String>,
    pub unit: Option<String>,
    pub created_by: Uuid,
}

pub async fn insert_product(pool: &PgPool, input: &NewProduct) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO products
            (name, sku, category_id, price, cost_price, stock_qty,
             low_stock_threshold, image_url, unit, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id
        "#,
        input.name,
        input.sku,
        input.category_id,
        input.price,
        input.cost_price,
        input.stock_qty,
        input.low_stock_threshold,
        input.image_url,
        input.unit,
        input.created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// Semua kolom opsional: yang `None` dibiarkan seperti semula. Stok TIDAK
/// ikut di sini -- satu-satunya jalan mengubah stok adalah lewat `stock.rs`,
/// supaya tidak ada perubahan yang lolos tanpa tercatat di ledger.
#[derive(Default)]
pub struct ProductPatch {
    pub name: Option<String>,
    pub sku: Option<String>,
    pub category_id: Option<Uuid>,
    pub price: Option<Decimal>,
    pub cost_price: Option<Decimal>,
    pub low_stock_threshold: Option<i32>,
    pub image_url: Option<String>,
    pub unit: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn update_product(pool: &PgPool, id: Uuid, patch: &ProductPatch) -> AppResult<bool> {
    let hasil = sqlx::query!(
        r#"
        UPDATE products SET
            name                = COALESCE($2, name),
            sku                 = COALESCE($3, sku),
            category_id         = COALESCE($4, category_id),
            price               = COALESCE($5, price),
            cost_price          = COALESCE($6, cost_price),
            low_stock_threshold = COALESCE($7, low_stock_threshold),
            image_url           = COALESCE($8, image_url),
            unit                = COALESCE($9, unit),
            is_active           = COALESCE($10, is_active),
            updated_at          = now()
        WHERE id = $1
        "#,
        id,
        patch.name.as_deref(),
        patch.sku.as_deref(),
        patch.category_id,
        patch.price,
        patch.cost_price,
        patch.low_stock_threshold,
        patch.image_url.as_deref(),
        patch.unit.as_deref(),
        patch.is_active
    )
    .execute(pool)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

pub async fn sku_dipakai(pool: &PgPool, sku: &str, kecuali: Option<Uuid>) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM products WHERE sku = $1 AND ($2::uuid IS NULL OR id <> $2)) AS "ada!""#,
        sku,
        kecuali
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

pub async fn category_ada(pool: &PgPool, id: Uuid) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM categories WHERE id = $1) AS "ada!""#,
        id
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

pub async fn list_categories(pool: &PgPool) -> AppResult<Vec<Category>> {
    let rows = sqlx::query_as!(
        Category,
        r#"SELECT id, name, created_by, created_at AS "created_at!" FROM categories ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn insert_category(pool: &PgPool, name: &str, created_by: Uuid) -> AppResult<Category> {
    let row = sqlx::query_as!(
        Category,
        r#"
        INSERT INTO categories (name, created_by)
        VALUES ($1, $2)
        RETURNING id, name, created_by, created_at AS "created_at!"
        "#,
        name,
        created_by
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn category_nama_dipakai(pool: &PgPool, name: &str) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM categories WHERE lower(name) = lower($1)) AS "ada!""#,
        name
    )
    .fetch_one(pool)
    .await?;

    Ok(ada)
}

pub async fn list_stock_adjustments(
    pool: &PgPool,
    product_id: Uuid,
    limit: i64,
) -> AppResult<Vec<StockAdjustment>> {
    let rows = sqlx::query_as!(
        StockAdjustment,
        r#"
        SELECT id, product_id, change_qty, reason, reference_type, reference_id,
               stock_before, stock_after, adjusted_by_user_id,
               created_at AS "created_at!"
        FROM stock_adjustments
        WHERE product_id = $1
        ORDER BY created_at DESC, id DESC
        LIMIT $2
        "#,
        product_id,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
