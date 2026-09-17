//! Akses tabel `customers`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Pelanggan toko beserta ringkasan belanjanya.
///
/// `name` boleh kosong sejak skema awal: pelanggan yang lahir dari impor
/// pesanan marketplace kadang hanya membawa username, bukan nama. Yang
/// menampilkan harus siap menghadapinya.
///
/// Angka belanja ikut di sini, tidak dipisah ke endpoint lain. Daftar
/// pelanggan tanpa "sudah belanja berapa" hampir tidak pernah cukup untuk
/// pertanyaan yang dibawa pemilik toko ke halaman ini, dan menghitungnya
/// di satu query jauh lebih murah daripada menarik seluruh transaksi ke
/// frontend lalu menjumlahkannya di sana.
///
/// OMZET ADALAH `subtotal`, BUKAN `total_amount` -- sama seperti di modul
/// laporan. Ongkir bukan belanja pelanggan atas barang.
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
    /// `belanja` = hanya yang pernah bertransaksi, `walk_in` / `marketplace`
    /// = asal pelanggan. Kosong berarti semua.
    pub scope: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// Urutan daftar. Nama untuk mencari orang, belanja untuk melihat siapa
/// yang paling berharga -- dua pertanyaan berbeda yang sama-sama dibawa ke
/// halaman ini.
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

/// Daftar pelanggan beserta jumlah seluruh baris yang cocok saringan.
///
/// Totalnya ikut dikembalikan supaya halaman bisa mengatakan "menampilkan
/// 25 dari 300", bukan diam-diam memotong daftar di batas `limit`.
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
                   SUM(t.subtotal)   AS total,
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
               (SELECT COALESCE(SUM(t.subtotal), 0) FROM transactions t
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

/// Pelanggan dengan nomor telepon tertentu, kalau ada.
///
/// Dipakai supaya pelanggan langganan yang kembali tidak melahirkan baris
/// baru setiap kali kasir mengetikkan namanya lagi. Memakai
/// `idx_customers_phone` yang sudah ada sejak skema awal.
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

/// `source` diisi tegas, bukan mengandalkan default kolom: pelanggan yang
/// dibuat di meja kasir memang walk-in, dan menuliskannya berarti perubahan
/// default kolom di kemudian hari tidak bisa diam-diam menandai ulang mereka.
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

    // Dibaca ulang lewat jalur yang sama dengan pembacaan lain supaya bentuk
    // yang dikembalikan tidak pernah berbeda dari yang dikembalikan daftar.
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound.into())
}

/// Bidang yang boleh diubah. `None` berarti "jangan sentuh", sedangkan
/// `Some(None)` berarti "kosongkan" -- dua hal yang berbeda, dan
/// membedakannya di tipe berarti tidak ada handler yang bisa salah
/// menghapus nomor telepon hanya karena form tidak mengirimkannya.
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

/// Foreign key di skema awal adalah `ON DELETE NO ACTION`, jadi Postgres
/// sudah menolak penghapusan pelanggan yang masih dirujuk. Dihitung di sini
/// supaya penolakannya bisa dijelaskan ("masih punya 3 transaksi"), bukan
/// muncul sebagai galat constraint yang tidak berarti apa-apa bagi kasir.
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
    pub product_id: Uuid,
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
        SELECT ti.product_id                 AS "product_id!",
               max(ti.product_name_snapshot) AS "name!",
               SUM(ti.qty)::bigint           AS "qty!",
               SUM(ti.subtotal)              AS "spent!"
        FROM transaction_items ti
        JOIN transactions t ON t.id = ti.transaction_id
        WHERE t.customer_id = $1 AND t.status = 'completed'
        GROUP BY ti.product_id
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
               t.subtotal           AS "revenue!",
               t.shipping_cost      AS "shipping!",
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
