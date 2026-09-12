//! Akses tabel `external_orders`, `external_order_items`, `channel_listings`,
//! dan `platforms`.

use crate::error::AppResult;
use crate::marketplace::NormalizedOrder;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct OrderItem {
    pub id: Uuid,
    pub product_id: Option<Uuid>,
    pub external_item_ref: Option<String>,
    pub item_name_snapshot: String,
    pub qty: i32,
    pub unit_price: Option<Decimal>,
    /// Nama produk internal hasil pemetaan, kalau sudah dipetakan.
    pub product_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub platform_id: Uuid,
    pub platform_name: String,
    pub external_order_id: String,
    pub status: String,
    pub sla_type: String,
    pub sla_deadline: Option<DateTime<Utc>>,
    pub total_amount: Option<Decimal>,
    pub shipping_carrier: Option<String>,
    pub received_at: DateTime<Utc>,
    /// Id tiket packing kalau sudah dibuat. Dipakai UI untuk tahu apakah
    /// order ini masih menunggu tiket.
    pub ticket_id: Option<Uuid>,
    pub items: Vec<OrderItem>,
}

struct OrderHead {
    id: Uuid,
    platform_id: Uuid,
    platform_name: String,
    external_order_id: String,
    status: String,
    sla_type: String,
    sla_deadline: Option<DateTime<Utc>>,
    total_amount: Option<Decimal>,
    shipping_carrier: Option<String>,
    received_at: DateTime<Utc>,
    ticket_id: Option<Uuid>,
}

async fn lengkapi(pool: &PgPool, head: OrderHead) -> AppResult<Order> {
    let items = sqlx::query_as!(
        OrderItem,
        r#"
        SELECT i.id, i.product_id, i.external_item_ref, i.item_name_snapshot,
               i.qty, i.unit_price, p.name AS "product_name?"
        FROM external_order_items i
        LEFT JOIN products p ON p.id = i.product_id
        WHERE i.external_order_id = $1
        ORDER BY i.created_at, i.id
        "#,
        head.id
    )
    .fetch_all(pool)
    .await?;

    Ok(Order {
        id: head.id,
        platform_id: head.platform_id,
        platform_name: head.platform_name,
        external_order_id: head.external_order_id,
        status: head.status,
        sla_type: head.sla_type,
        sla_deadline: head.sla_deadline,
        total_amount: head.total_amount,
        shipping_carrier: head.shipping_carrier,
        received_at: head.received_at,
        ticket_id: head.ticket_id,
        items,
    })
}

pub async fn list_orders(pool: &PgPool, status: Option<&str>, limit: i64) -> AppResult<Vec<Order>> {
    let heads = sqlx::query_as!(
        OrderHead,
        r#"
        SELECT o.id, o.platform_id, pl.platform_name, o.external_order_id,
               o.status, o.sla_type, o.sla_deadline, o.total_amount,
               o.shipping_carrier, o.received_at AS "received_at!",
               t.id AS "ticket_id?"
        FROM external_orders o
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN tickets t ON t.external_order_id = o.id
        WHERE ($1::text IS NULL OR o.status = $1)
        ORDER BY o.received_at DESC
        LIMIT $2
        "#,
        status,
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

pub async fn find_order(pool: &PgPool, id: Uuid) -> AppResult<Option<Order>> {
    let head = sqlx::query_as!(
        OrderHead,
        r#"
        SELECT o.id, o.platform_id, pl.platform_name, o.external_order_id,
               o.status, o.sla_type, o.sla_deadline, o.total_amount,
               o.shipping_carrier, o.received_at AS "received_at!",
               t.id AS "ticket_id?"
        FROM external_orders o
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN tickets t ON t.external_order_id = o.id
        WHERE o.id = $1
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

pub async fn find_platform_id(pool: &PgPool, platform_name: &str) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM platforms WHERE platform_name = $1",
        platform_name
    )
    .fetch_optional(pool)
    .await?;

    Ok(id)
}

#[derive(Debug, Serialize)]
pub struct PlatformStatus {
    pub id: Uuid,
    pub platform_name: String,
    pub is_connected: bool,
    pub shop_id_external: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub token_expires_at: Option<DateTime<Utc>>,
}

pub async fn list_platforms(pool: &PgPool) -> AppResult<Vec<PlatformStatus>> {
    let rows = sqlx::query_as!(
        PlatformStatus,
        r#"
        SELECT id, platform_name, is_connected, shop_id_external,
               last_synced_at, token_expires_at
        FROM platforms
        ORDER BY platform_name
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn disconnect_platform(pool: &PgPool, platform_name: &str) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE platforms SET
            is_connected            = false,
            access_token_encrypted  = NULL,
            refresh_token_encrypted = NULL,
            token_expires_at        = NULL,
            updated_at              = now()
        WHERE platform_name = $1
        "#,
        platform_name
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Hasil upsert: `dibuat` menandai order yang benar-benar baru, bukan
/// kiriman ulang webhook untuk order yang sudah ada.
pub struct HasilUpsert {
    pub id: Uuid,
    pub dibuat: bool,
}

/// Menyimpan satu order dari platform.
///
/// Kunci idempotensinya `(platform_id, external_order_id)` -- indeks unik
/// `idx_external_orders_platform_extid` yang menegakkannya. TikTok boleh
/// mengirim webhook yang sama berkali-kali (dan memang begitu kalau jawaban
/// kita terlambat); yang terjadi hanyalah baris yang sama diperbarui.
///
/// `received_at`, `sla_type`, dan `sla_deadline` sengaja TIDAK ditimpa saat
/// order sudah ada: tenggat packing dihitung dari saat order pertama kali
/// diterima, dan kiriman ulang tidak boleh memundurkan tenggat itu.
pub async fn upsert_order(
    pool: &PgPool,
    platform_id: Uuid,
    order: &NormalizedOrder,
    sla_type: &str,
    sla_deadline: DateTime<Utc>,
) -> AppResult<HasilUpsert> {
    let mut tx = pool.begin().await?;

    let baris = sqlx::query!(
        r#"
        INSERT INTO external_orders
            (platform_id, external_order_id, status, sla_type, sla_deadline,
             total_amount, payment_method, shipping_carrier, raw_payload)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (platform_id, external_order_id) DO UPDATE SET
            status           = EXCLUDED.status,
            total_amount     = EXCLUDED.total_amount,
            shipping_carrier = EXCLUDED.shipping_carrier,
            raw_payload      = EXCLUDED.raw_payload,
            updated_at       = now()
        RETURNING id, (xmax = 0) AS "dibuat!"
        "#,
        platform_id,
        order.external_order_id,
        order.status.as_str(),
        sla_type,
        sla_deadline,
        order.total_amount,
        order.payment_method,
        order.shipping_carrier,
        order.raw_payload
    )
    .fetch_one(&mut *tx)
    .await?;

    // Item ditulis ulang seluruhnya: order yang diperbarui bisa saja
    // kehilangan baris (pembeli membatalkan sebagian), dan menyisakan baris
    // lama akan membuat tiket packing meminta barang yang tidak jadi dibeli.
    // Aman dilakukan karena pemetaan produk disimpan di `channel_listings`,
    // bukan di baris item ini.
    sqlx::query!(
        "DELETE FROM external_order_items WHERE external_order_id = $1",
        baris.id
    )
    .execute(&mut *tx)
    .await?;

    for item in &order.items {
        sqlx::query!(
            r#"
            INSERT INTO external_order_items
                (external_order_id, product_id, external_item_ref,
                 item_name_snapshot, qty, unit_price)
            VALUES (
                $1,
                (SELECT product_id FROM channel_listings
                  WHERE platform_id = $2 AND external_item_id = $3),
                $3, $4, $5, $6
            )
            "#,
            baris.id,
            platform_id,
            item.external_item_ref,
            item.item_name,
            item.qty,
            item.unit_price
        )
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query!(
        "UPDATE platforms SET last_synced_at = now(), last_sync_status = 'success' WHERE id = $1",
        platform_id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(HasilUpsert {
        id: baris.id,
        dibuat: baris.dibuat,
    })
}

// ---------------------------------------------------------------------
// Pemetaan listing marketplace ke produk internal
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Mapping {
    pub id: Uuid,
    pub platform_id: Uuid,
    pub platform_name: String,
    pub product_id: Option<Uuid>,
    pub external_item_id: String,
    pub external_sku: Option<String>,
}

pub async fn list_mappings(pool: &PgPool, product_id: Uuid) -> AppResult<Vec<Mapping>> {
    let rows = sqlx::query_as!(
        Mapping,
        r#"
        SELECT cl.id, cl.platform_id, pl.platform_name, cl.product_id,
               cl.external_item_id, cl.external_sku
        FROM channel_listings cl
        JOIN platforms pl ON pl.id = cl.platform_id
        WHERE cl.product_id = $1
        ORDER BY pl.platform_name, cl.external_item_id
        "#,
        product_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Memetakan satu listing marketplace ke produk internal.
///
/// Setelah dipetakan, baris item order yang sudah terlanjur masuk tanpa
/// `product_id` ikut diperbaiki. Tanpa itu, order yang datang sebelum
/// pemetaan dibuat akan selamanya tidak bisa dibuatkan tiket packing.
pub async fn upsert_mapping(
    pool: &PgPool,
    platform_id: Uuid,
    product_id: Uuid,
    external_item_id: &str,
    external_sku: Option<&str>,
) -> AppResult<Uuid> {
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO channel_listings (platform_id, product_id, external_item_id, external_sku)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (platform_id, external_item_id) DO UPDATE SET
            product_id   = EXCLUDED.product_id,
            external_sku = EXCLUDED.external_sku,
            updated_at   = now()
        RETURNING id
        "#,
        platform_id,
        product_id,
        external_item_id,
        external_sku
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        UPDATE external_order_items i SET product_id = $2
        FROM external_orders o
        WHERE i.external_order_id = o.id
          AND o.platform_id = $1
          AND i.external_item_ref = $3
          AND i.product_id IS NULL
        "#,
        platform_id,
        product_id,
        external_item_id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(id)
}

pub async fn delete_mapping(pool: &PgPool, product_id: Uuid, mapping_id: Uuid) -> AppResult<bool> {
    let hasil = sqlx::query!(
        "DELETE FROM channel_listings WHERE id = $1 AND product_id = $2",
        mapping_id,
        product_id
    )
    .execute(pool)
    .await?;

    Ok(hasil.rows_affected() > 0)
}
