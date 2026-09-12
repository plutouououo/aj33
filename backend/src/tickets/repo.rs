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
    /// Rak tempat barang diambil, dibaca LANGSUNG dari produk -- bukan
    /// disalin ke `ticket_items` seperti namanya.
    ///
    /// Nama disalin karena nota lama harus tetap menyebut barang sebagaimana
    /// saat dipesan. Lokasi kebalikannya: pengepak butuh rak tempat barang
    /// berada SEKARANG. Salinan lama justru menyuruhnya ke rak yang salah
    /// begitu barang dipindah.
    pub storage_location: Option<String>,
}

/// Tiket beserta secuil data pesanannya.
///
/// Nomor pesanan, platform, dan tenggat SLA ikut dibawa karena antrean
/// packing dibaca dan diurutkan berdasarkan itu -- tanpanya frontend harus
/// memanggil `/orders/{id}` untuk tiap baris hanya demi menampilkan daftar.
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
               ti.is_packed, p.storage_location
        FROM ticket_items ti
        LEFT JOIN products p ON p.id = ti.product_id
        WHERE ti.ticket_id = $1
        -- Diurutkan per rak, bukan per nama: pengepak menyusuri gudang
        -- sekali jalan alih-alih bolak-balik. Barang tanpa lokasi jatuh ke
        -- bawah, supaya yang bisa dipandu tetap berurutan.
        ORDER BY p.storage_location ASC NULLS LAST, ti.product_name_snapshot, ti.id
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

/// Daftar tiket.
///
/// Urutannya `sla_deadline` menaik -- yang paling mendesak di atas, karena
/// itulah pertanyaan yang dijawab layar pengepak: mana yang harus dikerjakan
/// sekarang. Tiket tanpa tenggat jatuh ke bawah.
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

/// Baris pesanan yang akan menjadi isi tiket. `product_id` kosong berarti
/// listing marketplace-nya belum dipetakan ke produk internal.
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

/// Membuat tiket. Mengembalikan `None` kalau pesanan ini sudah punya tiket
/// -- dijaga indeks unik `idx_tickets_external_order`, jadi dua permintaan
/// yang datang bersamaan tetap hanya menghasilkan satu tiket.
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
) -> AppResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO ticket_items (ticket_id, product_id, product_name_snapshot, qty)
        VALUES ($1, $2, $3, $4)
        "#,
        ticket_id,
        product_id,
        product_name_snapshot,
        qty
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Apakah user ini pantas memegang tiket packing: masih aktif, dan
/// perannya pengepak (atau owner, yang di toko kecil ikut mengepak).
/// Foreign key hanya menjamin user-nya ada -- bukan bahwa dia bekerja di
/// bagian packing.
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

/// Membaca tiket sambil menguncinya sampai transaksi pemanggil selesai.
///
/// Semua perubahan status lewat sini lebih dulu. Tanpa kunci, dua permintaan
/// "serahkan" yang datang bersamaan sama-sama membaca status `packed`,
/// sama-sama lolos pemeriksaan, dan stok berkurang dua kali.
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

/// Menulis status baru. `completed_at` diisi hanya saat serah-terima --
/// itulah saat pekerjaan tiket benar-benar selesai.
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
}

pub async fn baris_stok(
    tx: &mut Transaction<'_, Postgres>,
    ticket_id: Uuid,
) -> AppResult<Vec<BarisStok>> {
    let rows = sqlx::query_as!(
        BarisStok,
        "SELECT product_id, qty FROM ticket_items WHERE ticket_id = $1",
        ticket_id
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}
