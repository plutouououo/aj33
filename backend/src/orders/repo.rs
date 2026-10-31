//! Akses tabel `external_orders`, `external_order_items`, `channel_listings`, dan `platforms`.

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
    pub model_sku: Option<String>,
    pub model_name: Option<String>,
    /// Nama produk internal hasil pemetaan, kalau sudah dipetakan.
    pub product_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub platform_id: Uuid,
    pub platform_name: String,
    pub external_order_id: String,
    pub pickup_code: Option<String>,
    pub status: String,
    pub sla_type: String,
    pub sla_deadline: Option<DateTime<Utc>>,
    pub total_amount: Option<Decimal>,
    pub shipping_carrier: Option<String>,
    pub payment_method: Option<String>,
    pub is_cod: bool,
    pub received_at: DateTime<Utc>,
    /// Id tiket packing bila sudah dibuat, dipakai UI untuk tahu apakah order masih menunggu tiket.
    pub ticket_id: Option<Uuid>,
    pub recipient_name: Option<String>,
    pub recipient_address: Option<String>,
    pub recipient_city: Option<String>,
    pub recipient_district: Option<String>,
    pub items: Vec<OrderItem>,
}

struct OrderHead {
    id: Uuid,
    platform_id: Uuid,
    platform_name: String,
    external_order_id: String,
    pickup_code: Option<String>,
    status: String,
    sla_type: String,
    sla_deadline: Option<DateTime<Utc>>,
    total_amount: Option<Decimal>,
    shipping_carrier: Option<String>,
    payment_method: Option<String>,
    is_cod: bool,
    received_at: DateTime<Utc>,
    ticket_id: Option<Uuid>,
    recipient_name: Option<String>,
    recipient_address: Option<String>,
    recipient_city: Option<String>,
    recipient_district: Option<String>,
}

async fn lengkapi(pool: &PgPool, head: OrderHead) -> AppResult<Order> {
    let items = sqlx::query_as!(
        OrderItem,
        r#"
        SELECT i.id, i.product_id, i.external_item_ref, i.item_name_snapshot,
               i.qty, i.unit_price, i.model_sku, i.model_name, p.name AS "product_name?"
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
        pickup_code: head.pickup_code,
        status: head.status,
        sla_type: head.sla_type,
        sla_deadline: head.sla_deadline,
        total_amount: head.total_amount,
        shipping_carrier: head.shipping_carrier,
        payment_method: head.payment_method,
        is_cod: head.is_cod,
        received_at: head.received_at,
        ticket_id: head.ticket_id,
        recipient_name: head.recipient_name,
        recipient_address: head.recipient_address,
        recipient_city: head.recipient_city,
        recipient_district: head.recipient_district,
        items,
    })
}

/// `tanpa_tiket` menyaring ke pesanan yang belum dikirim ke pengepak, antrean halaman Pesanan Masuk; terbaru di atas karena owner mencari label yang baru diunggahnya.
pub async fn list_orders(
    pool: &PgPool,
    status: Option<&str>,
    tanpa_tiket: bool,
    limit: i64,
) -> AppResult<Vec<Order>> {
    let heads = sqlx::query_as!(
        OrderHead,
        r#"
        SELECT o.id, o.platform_id, pl.platform_name, o.external_order_id, o.pickup_code,
               o.status, o.sla_type, o.sla_deadline, o.total_amount,
               o.shipping_carrier, o.payment_method, o.is_cod,
               o.received_at AS "received_at!",
               t.id AS "ticket_id?",
               a.recipient_name AS "recipient_name?", a.full_address AS "recipient_address?",
               a.city AS "recipient_city?", a.district AS "recipient_district?"
        FROM external_orders o
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN tickets t ON t.external_order_id = o.id
        LEFT JOIN LATERAL (
            SELECT recipient_name, full_address, city, district
            FROM order_shipping_address
            WHERE external_order_id = o.id
            ORDER BY created_at DESC
            LIMIT 1
        ) a ON true
        WHERE ($1::text IS NULL OR o.status = $1)
          AND (NOT $2::bool OR t.id IS NULL)
        ORDER BY o.received_at DESC, o.id
        LIMIT $3
        "#,
        status,
        tanpa_tiket,
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
        SELECT o.id, o.platform_id, pl.platform_name, o.external_order_id, o.pickup_code,
               o.status, o.sla_type, o.sla_deadline, o.total_amount,
               o.shipping_carrier, o.payment_method, o.is_cod,
               o.received_at AS "received_at!",
               t.id AS "ticket_id?",
               a.recipient_name AS "recipient_name?", a.full_address AS "recipient_address?",
               a.city AS "recipient_city?", a.district AS "recipient_district?"
        FROM external_orders o
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN tickets t ON t.external_order_id = o.id
        LEFT JOIN LATERAL (
            SELECT recipient_name, full_address, city, district
            FROM order_shipping_address
            WHERE external_order_id = o.id
            ORDER BY created_at DESC
            LIMIT 1
        ) a ON true
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

/// Hasil upsert: `dibuat` menandai order benar-benar baru, bukan kiriman ulang webhook.
pub struct HasilUpsert {
    pub id: Uuid,
    pub dibuat: bool,
}

/// Upsert order dengan kunci idempotensi `(platform_id, external_order_id)` karena webhook bisa berulang; `received_at`/`sla_*` tak ditimpa agar tenggat tak mundur.
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

    // Item ditulis ulang seluruhnya agar baris yang dibatalkan sebagian tak membuat tiket meminta barang tak jadi dibeli; aman karena pemetaan ada di `channel_listings`.
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

// --- Pemetaan listing marketplace ke produk internal ---

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

/// Memetakan satu listing marketplace ke produk internal; baris item yang terlanjur tanpa `product_id` ikut diperbaiki, kalau tidak order sebelum pemetaan tak bisa dibuatkan tiket.
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

/// Hanya pesanan baru yang belum punya tugas packing yang dibatalkan; yang sudah ditugaskan harus lewat alur tiket agar stok dan pengepaknya tak menggantung.
pub async fn batalkan_pesanan(pool: &PgPool, ids: &[Uuid]) -> AppResult<u64> {
    let hasil = sqlx::query!(
        r#"
        UPDATE external_orders o
        SET status = 'cancelled', updated_at = now()
        WHERE o.id = ANY($1)
          AND o.status = 'new'
          AND NOT EXISTS (SELECT 1 FROM tickets t WHERE t.external_order_id = o.id)
        "#,
        ids
    )
    .execute(pool)
    .await?;

    Ok(hasil.rows_affected())
}

// --- Pesanan dari label pengiriman Shopee ---

/// Produk jual dengan SKU ini; produk induk bervarian dilewati karena stoknya hidup di varian.
pub async fn produk_dari_sku(pool: &PgPool, sku: &str) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar!(
        r#"
        SELECT p.id
        FROM products p
        WHERE upper(p.sku) = upper($1)
          AND p.is_active
          AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id)
        "#,
        sku
    )
    .fetch_optional(pool)
    .await?;

    Ok(id)
}

pub struct LabelBarisBaru {
    pub product_id: Uuid,
    pub name: String,
    pub sku: String,
    pub variation: Option<String>,
    pub qty: i32,
}

pub struct LabelBaru<'a> {
    pub platform_id: Uuid,
    pub order_sn: &'a str,
    pub sla_type: &'a str,
    pub sla_deadline: DateTime<Utc>,
    pub payment_label: &'a str,
    pub is_cod: bool,
    pub shipping_carrier: &'a str,
    pub recipient_name: &'a str,
    pub address: &'a str,
    pub city: &'a str,
    pub district: &'a str,
    pub pickup_code: Option<&'a str>,
    pub raw_payload: serde_json::Value,
    pub items: &'a [LabelBarisBaru],
}

/// Mencatat pesanan, penerimanya, dan barangnya sekaligus; `None` bila nomor pesanan sudah ada, karena label yang sama sering terunggah ulang dan pesanan lama mungkin sudah bertiket.
pub async fn insert_pesanan_label(pool: &PgPool, label: &LabelBaru<'_>) -> AppResult<Option<Uuid>> {
    let mut tx = pool.begin().await?;

    let Some(order_id) = sqlx::query_scalar!(
        r#"
        INSERT INTO external_orders
            (platform_id, external_order_id, status, sla_type, sla_deadline,
             payment_method, is_cod, shipping_carrier, raw_payload, pickup_code)
        VALUES ($1, $2, 'new', $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (platform_id, external_order_id) DO NOTHING
        RETURNING id
        "#,
        label.platform_id,
        label.order_sn,
        label.sla_type,
        label.sla_deadline,
        label.payment_label,
        label.is_cod,
        label.shipping_carrier,
        label.raw_payload,
        label.pickup_code
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };

    // Label tak memuat telepon penerima, jadi pembeli yang sama dikenali dari nama dan alamatnya.
    let customer_id = match sqlx::query_scalar!(
        r#"
        SELECT id FROM customers
        WHERE source = 'marketplace' AND name = $1 AND address = $2
        ORDER BY created_at
        LIMIT 1
        "#,
        label.recipient_name,
        label.address
    )
    .fetch_optional(&mut *tx)
    .await?
    {
        Some(id) => {
            sqlx::query!(
                "UPDATE customers SET city = $2, district = $3, updated_at = now() WHERE id = $1",
                id,
                label.city,
                label.district
            )
            .execute(&mut *tx)
            .await?;
            id
        }
        None => {
            sqlx::query_scalar!(
                r#"
                INSERT INTO customers (name, address, city, district, source)
                VALUES ($1, $2, $3, $4, 'marketplace')
                RETURNING id
                "#,
                label.recipient_name,
                label.address,
                label.city,
                label.district
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };

    sqlx::query!(
        "UPDATE external_orders SET customer_id = $2 WHERE id = $1",
        order_id,
        customer_id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        INSERT INTO order_shipping_address
            (external_order_id, recipient_name, full_address, city, district)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        order_id,
        label.recipient_name,
        label.address,
        label.city,
        label.district
    )
    .execute(&mut *tx)
    .await?;

    for item in label.items {
        sqlx::query!(
            r#"
            INSERT INTO external_order_items
                (external_order_id, product_id, item_name_snapshot, qty, model_sku, model_name)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            order_id,
            item.product_id,
            item.name,
            item.qty,
            item.sku,
            item.variation
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Some(order_id))
}
