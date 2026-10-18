//! Akses tabel `tickets` dan `ticket_items`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct TicketItem {
    pub id: Uuid,
    pub product_id: Uuid,
    pub product_name_snapshot: String,
    pub qty: i32,
    pub is_packed: bool,
    /// Batch pilihan owner saat mengirim tiket; `None` berarti FEFO saat tiket selesai.
    pub batch_id: Option<Uuid>,
    pub batch_number: Option<String>,
    pub batch_expiry: Option<chrono::NaiveDate>,
    /// Rak batch pilihan, atau tanpa pilihan dibaca dari batch bersisa urut FEFO; dibaca langsung (bukan salinan) karena lokasi harus yang SEKARANG.
    pub storage_location: Option<String>,
}

/// Tiket beserta secuil data pesanan (nomor, platform, tenggat SLA) karena antrean diurutkan berdasarkan itu, tanpa memanggil `/orders/{id}` per baris.
#[derive(Debug, Serialize)]
pub struct Ticket {
    pub id: Uuid,
    pub external_order_id: Uuid,
    pub order_ref: String,
    pub platform_name: String,
    pub sla_type: String,
    pub sla_deadline: Option<DateTime<Utc>>,
    pub status: String,
    pub assigned_to_user_id: Option<Uuid>,
    pub assigned_to_name: Option<String>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub items: Vec<TicketItem>,
}

struct TicketHead {
    id: Uuid,
    external_order_id: Uuid,
    order_ref: String,
    platform_name: String,
    sla_type: String,
    sla_deadline: Option<DateTime<Utc>>,
    status: String,
    assigned_to_user_id: Option<Uuid>,
    assigned_to_name: Option<String>,
    assigned_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    notes: Option<String>,
}

async fn lengkapi(pool: &PgPool, head: TicketHead) -> AppResult<Ticket> {
    let items = sqlx::query_as!(
        TicketItem,
        r#"
        SELECT ti.id, ti.product_id, ti.product_name_snapshot, ti.qty,
               ti.is_packed, ti.batch_id,
               pb.batch_number AS "batch_number?", pb.expiry_date AS "batch_expiry?",
               CASE WHEN ti.batch_id IS NOT NULL THEN pb.storage_location ELSE (
                   -- Rak yang masih berisi, tanpa pengulangan dan urut FEFO (batch habis tak ikut); dikelompokkan per rak dulu karena `string_agg DISTINCT` tak menjamin urutan ORDER BY subkueri.
                   SELECT string_agg(rak.storage_location, ', '
                                     ORDER BY rak.exp NULLS LAST, rak.masuk)
                   FROM (
                       SELECT b.storage_location    AS storage_location,
                              min(b.expiry_date)    AS exp,
                              min(b.received_at)    AS masuk
                       FROM product_batches b
                       WHERE b.product_id = ti.product_id
                         AND b.remaining_qty > 0
                         AND b.storage_location IS NOT NULL
                       GROUP BY b.storage_location
                   ) rak
               ) END AS storage_location
        FROM ticket_items ti
        LEFT JOIN product_batches pb ON pb.id = ti.batch_id
        WHERE ti.ticket_id = $1
        -- Diurutkan per rak, bukan per nama, agar pengepak menyusuri gudang sekali jalan; barang tanpa lokasi jatuh ke bawah.
        ORDER BY 9 ASC NULLS LAST, ti.product_name_snapshot, ti.id
        "#,
        head.id
    )
    .fetch_all(pool)
    .await?;

    Ok(Ticket {
        id: head.id,
        external_order_id: head.external_order_id,
        order_ref: head.order_ref,
        platform_name: head.platform_name,
        sla_type: head.sla_type,
        sla_deadline: head.sla_deadline,
        status: head.status,
        assigned_to_user_id: head.assigned_to_user_id,
        assigned_to_name: head.assigned_to_name,
        assigned_at: head.assigned_at,
        completed_at: head.completed_at,
        notes: head.notes,
        items,
    })
}

/// Daftar tiket urut `sla_deadline` menaik (paling mendesak di atas, pertanyaan layar pengepak: mana dikerjakan sekarang); tanpa tenggat jatuh ke bawah.
pub async fn list(
    pool: &PgPool,
    status: Option<&str>,
    assigned_to: Option<Uuid>,
) -> AppResult<Vec<Ticket>> {
    let heads = sqlx::query_as!(
        TicketHead,
        r#"
        SELECT t.id, t.external_order_id, o.external_order_id AS order_ref,
               pl.platform_name, o.sla_type, o.sla_deadline, t.status,
               t.assigned_to_user_id, u.name AS "assigned_to_name?",
               t.assigned_at, t.completed_at, t.notes
        FROM tickets t
        JOIN external_orders o ON o.id = t.external_order_id
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN users u ON u.id = t.assigned_to_user_id
        WHERE ($1::text IS NULL OR t.status = $1)
          AND ($2::uuid IS NULL OR t.assigned_to_user_id = $2)
        ORDER BY o.sla_deadline ASC NULLS LAST, t.created_at ASC
        LIMIT 100
        "#,
        status,
        assigned_to
    )
    .fetch_all(pool)
    .await?;

    let mut hasil = Vec::with_capacity(heads.len());
    for head in heads {
        hasil.push(lengkapi(pool, head).await?);
    }

    Ok(hasil)
}

pub async fn find(pool: &PgPool, id: Uuid) -> AppResult<Option<Ticket>> {
    let head = sqlx::query_as!(
        TicketHead,
        r#"
        SELECT t.id, t.external_order_id, o.external_order_id AS order_ref,
               pl.platform_name, o.sla_type, o.sla_deadline, t.status,
               t.assigned_to_user_id, u.name AS "assigned_to_name?",
               t.assigned_at, t.completed_at, t.notes
        FROM tickets t
        JOIN external_orders o ON o.id = t.external_order_id
        JOIN platforms pl ON pl.id = o.platform_id
        LEFT JOIN users u ON u.id = t.assigned_to_user_id
        WHERE t.id = $1
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

/// Baris pesanan yang menjadi isi tiket; `product_id` kosong berarti listing marketplace belum dipetakan ke produk.
pub struct BarisPesanan {
    pub product_id: Option<Uuid>,
    pub product_name: Option<String>,
    pub item_name_snapshot: String,
    pub qty: i32,
}

pub async fn baris_pesanan(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
) -> AppResult<Vec<BarisPesanan>> {
    let rows = sqlx::query_as!(
        BarisPesanan,
        r#"
        SELECT i.product_id, p.name AS "product_name?", i.item_name_snapshot, i.qty
        FROM external_order_items i
        LEFT JOIN products p ON p.id = i.product_id
        WHERE i.external_order_id = $1
        ORDER BY i.created_at, i.id
        "#,
        external_order_id
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}

pub async fn ada_pesanan(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM external_orders WHERE id = $1)",
        external_order_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(ada.unwrap_or(false))
}

/// Membuat tiket; `None` bila pesanan sudah punya tiket (dijaga indeks unik `idx_tickets_external_order`) sehingga dua permintaan bersamaan hanya menghasilkan satu.
pub async fn insert_ticket(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
    notes: Option<&str>,
) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO tickets (external_order_id, status, notes)
        VALUES ($1, 'unassigned', $2)
        ON CONFLICT (external_order_id) DO NOTHING
        RETURNING id
        "#,
        external_order_id,
        notes
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(id)
}

pub async fn insert_item(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
    product_id: Uuid,
    product_name_snapshot: &str,
    qty: i32,
    batch_id: Option<Uuid>,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO ticket_items (ticket_id, product_id, product_name_snapshot, qty, batch_id)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        ticket_id,
        product_id,
        product_name_snapshot,
        qty,
        batch_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Apakah user pantas memegang tiket: aktif dan berperan pengepak (atau owner yang ikut mengepak); foreign key hanya menjamin user ada.
pub async fn bisa_mengepak(tx: &mut Transaction<'_, Postgres>, user_id: Uuid) -> AppResult<bool> {
    let bisa = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM users
            WHERE id = $1 AND is_active AND role IN ('pengepak', 'owner')
        )
        "#,
        user_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(bisa.unwrap_or(false))
}

pub struct TiketTerkunci {
    pub external_order_id: Uuid,
    pub status: String,
    pub assigned_to_user_id: Option<Uuid>,
}

/// Membaca tiket sambil menguncinya sampai transaksi pemanggil selesai; semua perubahan status lewat sini, kalau tidak dua "serahkan" bersamaan sama-sama lolos dan stok berkurang dua kali.
pub async fn kunci(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<Option<TiketTerkunci>> {
    let row = sqlx::query_as!(
        TiketTerkunci,
        r#"
        SELECT external_order_id, status, assigned_to_user_id
        FROM tickets
        WHERE id = $1
        FOR UPDATE
        "#,
        id
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row)
}

pub async fn set_penugasan(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    assigned_to: Uuid,
    assigned_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE tickets SET
            assigned_to_user_id = $2,
            assigned_by         = $3,
            assigned_at         = now(),
            status              = 'assigned',
            updated_at          = now()
        WHERE id = $1
        "#,
        id,
        assigned_to,
        assigned_by
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Menulis status baru; `completed_at` hanya saat serah-terima karena itulah pekerjaan tiket selesai.
pub async fn set_status(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    status: &str,
    selesai: bool,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE tickets SET
            status       = $2,
            completed_at = CASE WHEN $3 THEN now() ELSE completed_at END,
            updated_at   = now()
        WHERE id = $1
        "#,
        id,
        status,
        selesai
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Mengembalikan `false` kalau barisnya bukan milik tiket ini.
pub async fn set_item_terkemas(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
    item_id: Uuid,
    is_packed: bool,
) -> AppResult<bool> {
    let hasil = sqlx::query!(
        "UPDATE ticket_items SET is_packed = $3 WHERE id = $2 AND ticket_id = $1",
        ticket_id,
        item_id,
        is_packed
    )
    .execute(&mut **tx)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

pub async fn ada_yang_belum_dikemas(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM ticket_items WHERE ticket_id = $1 AND NOT is_packed)",
        ticket_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(ada.unwrap_or(false))
}

pub struct BarisStok {
    pub product_id: Uuid,
    pub qty: i32,
    pub batch_id: Option<Uuid>,
}

pub async fn baris_stok(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
) -> AppResult<Vec<BarisStok>> {
    let rows = sqlx::query_as!(
        BarisStok,
        "SELECT product_id, qty, batch_id FROM ticket_items WHERE ticket_id = $1",
        ticket_id
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}

pub async fn batch_milik_produk(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    product_id: Uuid,
) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM product_batches WHERE id = $1 AND product_id = $2)",
        batch_id,
        product_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(ada.unwrap_or(false))
}

pub async fn set_status_pesanan(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
    status: &str,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE external_orders SET status = $2, updated_at = now() WHERE id = $1",
        external_order_id,
        status
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub struct InfoPesanan {
    pub platform_name: String,
    pub customer_id: Option<Uuid>,
}

pub async fn info_pesanan(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
) -> AppResult<InfoPesanan> {
    let row = sqlx::query_as!(
        InfoPesanan,
        r#"
        SELECT pl.platform_name, o.customer_id
        FROM external_orders o
        JOIN platforms pl ON pl.id = o.platform_id
        WHERE o.id = $1
        "#,
        external_order_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(row)
}

/// Pesanan yang barangnya sudah keluar ditautkan ke penjualannya, agar dari pesanan bisa ditelusuri transaksi mana yang mengurangi stoknya.
pub async fn tautkan_penjualan(
    tx: &mut Transaction<'_, Postgres>,
    external_order_id: Uuid,
    transaction_id: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE external_orders
        SET status = 'shipped', transaction_id = $2, updated_at = now()
        WHERE id = $1
        "#,
        external_order_id,
        transaction_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}
