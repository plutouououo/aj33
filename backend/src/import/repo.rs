//! Akses tabel `import_batches`, `import_rows`, `import_aliases`.

use crate::error::AppResult;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ImportBatch {
    pub id: Uuid,
    pub file_name: String,
    pub status: String,
    pub total_rows: i32,
    pub publish_on_commit: bool,
    pub uploaded_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_note: Option<String>,
    pub committed_at: Option<DateTime<Utc>>,
    pub ok_count: i32,
    pub fail_count: i32,
}

/// Hitungan per kategori baris -- ditampilkan di panel ringkasan halaman
/// review, dan `error_count > 0` adalah gerbang submit/approve (lihat
/// `ada_error_row`, query terpisah supaya bisa dipanggil tanpa memuat
/// seluruh baris).
#[derive(Debug, Serialize)]
pub struct RingkasanBaris {
    pub create_count: i64,
    pub update_count: i64,
    pub skip_count: i64,
    pub error_count: i64,
    pub warn_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ImportRow {
    pub id: Uuid,
    pub row_no: i32,
    pub name: Option<String>,
    pub sku: Option<String>,
    pub category_text: Option<String>,
    pub brand_text: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<String>,
    pub category_id: Option<Uuid>,
    pub lowest_price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub stock: Option<i32>,
    pub published: Option<bool>,
    pub action: String,
    pub match_product_id: Option<Uuid>,
    pub issues: serde_json::Value,
    pub commit_state: String,
    pub commit_product_id: Option<Uuid>,
    pub commit_error: Option<String>,
}

pub struct NewImportRow {
    pub row_no: i32,
    pub raw: serde_json::Value,
    pub name: Option<String>,
    pub sku: Option<String>,
    pub category_text: Option<String>,
    pub brand_text: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<String>,
    pub category_id: Option<Uuid>,
    pub lowest_price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub stock: Option<i32>,
    pub published: Option<bool>,
    pub action: String,
    pub match_product_id: Option<Uuid>,
    pub issues: serde_json::Value,
}

pub async fn insert_batch(
    tx: &mut Transaction<'_, Postgres>,
    file_name: &str,
    file_sha256: &str,
    total_rows: i32,
    uploaded_by: Uuid,
) -> AppResult<Uuid> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO import_batches (file_name, file_sha256, total_rows, uploaded_by)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        file_name,
        file_sha256,
        total_rows,
        uploaded_by
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(id)
}

/// Loop `INSERT` satu per baris di dalam SATU transaksi. Muat sampai 5000
/// baris sekali jalan -- cukup untuk batas atas fitur ini (lihat
/// `parse::BATAS_BARIS`); kalau nanti perlu lebih cepat, ganti jadi satu
/// `INSERT ... SELECT * FROM UNNEST(...)`, bukan sebelum ada bukti perlu.
pub async fn insert_rows(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    rows: &[NewImportRow],
) -> AppResult<()> {
    for r in rows {
        sqlx::query!(
            r#"
            INSERT INTO import_rows
                (batch_id, row_no, raw, name, sku, category_text, brand_text,
                 product_type, variant_grade, variant_size, category_id,
                 lowest_price, cost, margin_pct, stock, published, action,
                 match_product_id, issues)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                    $14, $15, $16, $17, $18, $19)
            "#,
            batch_id,
            r.row_no,
            r.raw,
            r.name,
            r.sku,
            r.category_text,
            r.brand_text,
            r.product_type,
            r.variant_grade,
            r.variant_size,
            r.category_id,
            r.lowest_price,
            r.cost,
            r.margin_pct,
            r.stock,
            r.published,
            r.action,
            r.match_product_id,
            r.issues,
        )
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

pub async fn find_batch(pool: &PgPool, id: Uuid) -> AppResult<Option<ImportBatch>> {
    let row = sqlx::query_as!(
        ImportBatch,
        r#"
        SELECT id, file_name, status, total_rows, publish_on_commit,
               uploaded_at AS "uploaded_at!", submitted_at, reviewed_at,
               review_note, committed_at, ok_count, fail_count
        FROM import_batches
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn list_batches(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> AppResult<(Vec<ImportBatch>, i64)> {
    let data = sqlx::query_as!(
        ImportBatch,
        r#"
        SELECT id, file_name, status, total_rows, publish_on_commit,
               uploaded_at AS "uploaded_at!", submitted_at, reviewed_at,
               review_note, committed_at, ok_count, fail_count
        FROM import_batches
        ORDER BY uploaded_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM import_batches"#)
        .fetch_one(pool)
        .await?;

    Ok((data, total))
}

/// Baris `action='skip'` DIKELUARKAN dari `error_count`/`warn_count` --
/// baris yang sengaja dilewati tidak lagi "perlu perhatian", apa pun isu
/// yang pernah tercatat sebelum dilewati (`issues`-nya tidak dihapus, cuma
/// tidak lagi dihitung). Sama gerbang yang dipakai `ada_error_row` di bawah.
pub async fn hitung_ringkasan(pool: &PgPool, batch_id: Uuid) -> AppResult<RingkasanBaris> {
    let baris = sqlx::query_as!(
        RingkasanBaris,
        r#"
        SELECT
            count(*) FILTER (WHERE action = 'create')                        AS "create_count!",
            count(*) FILTER (WHERE action = 'update')                        AS "update_count!",
            count(*) FILTER (WHERE action = 'skip')                          AS "skip_count!",
            count(*) FILTER (
                WHERE action <> 'skip' AND EXISTS (
                    SELECT 1 FROM jsonb_array_elements(issues) e
                    WHERE e->>'level' = 'error'
                )
            )                                                                AS "error_count!",
            count(*) FILTER (
                WHERE action <> 'skip' AND EXISTS (
                    SELECT 1 FROM jsonb_array_elements(issues) e
                    WHERE e->>'level' = 'warn'
                )
            )                                                                AS "warn_count!"
        FROM import_rows
        WHERE batch_id = $1
        "#,
        batch_id
    )
    .fetch_one(pool)
    .await?;

    Ok(baris)
}

/// Gerbang submit/approve SESUNGGUHNYA -- dipanggil di dalam transaksi yang
/// sama dengan perubahan status, supaya tidak ada baris ERROR baru yang
/// menyelinap masuk di antara pemeriksaan dan penguncian. Baris yang sudah
/// `action='skip'` TIDAK dihitung -- itulah gunanya skip: melewati baris
/// bermasalah tanpa memperbaiki berkasnya.
pub async fn ada_error_row(tx: &mut Transaction<'_, Postgres>, batch_id: Uuid) -> AppResult<bool> {
    let ada = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM import_rows r, jsonb_array_elements(r.issues) e
            WHERE r.batch_id = $1 AND r.action <> 'skip' AND e->>'level' = 'error'
        ) AS "ada!"
        "#,
        batch_id
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(ada)
}

pub async fn list_rows(
    pool: &PgPool,
    batch_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<(Vec<ImportRow>, i64)> {
    let data = sqlx::query_as!(
        ImportRow,
        r#"
        SELECT id, row_no, name, sku, category_text, brand_text, product_type,
               variant_grade, variant_size, category_id, lowest_price, cost,
               margin_pct, stock, published, action, match_product_id,
               issues, commit_state, commit_product_id, commit_error
        FROM import_rows
        WHERE batch_id = $1
        ORDER BY row_no
        LIMIT $2 OFFSET $3
        "#,
        batch_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM import_rows WHERE batch_id = $1"#,
        batch_id
    )
    .fetch_one(pool)
    .await?;

    Ok((data, total))
}

pub struct ImportBatchTerkunci {
    pub status: String,
    pub publish_on_commit: bool,
    pub uploaded_by: Option<Uuid>,
    pub fail_count: i32,
}

/// Membaca batch sambil menguncinya sampai transaksi pemanggil selesai --
/// sama pola dengan `tickets::repo::kunci`. Tanpa kunci ini, dua klik
/// "submit"/"approve" yang datang bersamaan sama-sama membaca status lama,
/// sama-sama lolos pemeriksaan, dan berpindah status dua kali.
pub async fn kunci_batch(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> AppResult<Option<ImportBatchTerkunci>> {
    let row = sqlx::query_as!(
        ImportBatchTerkunci,
        r#"SELECT status, publish_on_commit, uploaded_by, fail_count FROM import_batches WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row)
}

pub async fn set_status(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    status: &str,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE import_batches SET status = $2 WHERE id = $1",
        id,
        status
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn tandai_disubmit(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<()> {
    sqlx::query!(
        "UPDATE import_batches SET status = 'pending_review', submitted_at = now() WHERE id = $1",
        id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn setujui_dan_mulai_commit(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    reviewed_by: Uuid,
    review_note: Option<&str>,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_batches
        SET status = 'committing', reviewed_by = $2, reviewed_at = now(), review_note = $3
        WHERE id = $1
        "#,
        id,
        reviewed_by,
        review_note
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn set_publish_on_commit(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    nilai: bool,
) -> AppResult<()> {
    sqlx::query!(
        "UPDATE import_batches SET publish_on_commit = $2 WHERE id = $1",
        id,
        nilai
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// `action` baris yang di-skip TETAP tersimpan (mis. `create`/`update`
/// aslinya) -- hanya `commit_state` yang berubah, supaya `restore_row` bisa
/// mengembalikannya tanpa perlu menghitung ulang match SKU.
pub async fn skip_row(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    row_id: Uuid,
) -> AppResult<bool> {
    let hasil = sqlx::query!(
        r#"
        UPDATE import_rows SET action = 'skip'
        WHERE id = $1 AND batch_id = $2 AND action <> 'skip'
        "#,
        row_id,
        batch_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

pub async fn restore_row(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    row_id: Uuid,
) -> AppResult<bool> {
    let hasil = sqlx::query!(
        r#"
        UPDATE import_rows
        SET action = CASE WHEN match_product_id IS NULL THEN 'create' ELSE 'update' END
        WHERE id = $1 AND batch_id = $2 AND action = 'skip'
        "#,
        row_id,
        batch_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(hasil.rows_affected() > 0)
}

/// SATU query untuk seluruh SKU unik dalam berkas -- bukan satu per baris.
/// Cocok case-insensitive & tanpa spasi tepi, sama seperti syarat dedup di
/// `parse::validate_row`.
pub async fn cari_produk_by_sku(
    pool: &PgPool,
    sku_list: &[String],
) -> AppResult<HashMap<String, Uuid>> {
    if sku_list.is_empty() {
        return Ok(HashMap::new());
    }

    struct Baris {
        sku_key: String,
        id: Uuid,
    }

    let rows = sqlx::query_as!(
        Baris,
        r#"
        SELECT lower(trim(sku)) AS "sku_key!", id
        FROM products
        WHERE lower(trim(sku)) = ANY($1)
        "#,
        sku_list
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| (r.sku_key, r.id)).collect())
}

/// Resolusi teks kategori -> `category_id`: cocok nama persis
/// (case-insensitive) dulu, baru jatuh ke `import_aliases` yang pernah
/// diajarkan. Dua tahap dalam satu fungsi karena keduanya dipakai bersama
/// tiap kali sebuah batch diunggah.
pub async fn resolusi_kategori_banyak(
    pool: &PgPool,
    teks_list: &[String],
) -> AppResult<HashMap<String, Uuid>> {
    if teks_list.is_empty() {
        return Ok(HashMap::new());
    }

    struct Baris {
        kunci: String,
        id: Uuid,
    }

    let mut hasil = HashMap::new();

    let dari_nama = sqlx::query_as!(
        Baris,
        r#"
        SELECT lower(trim(name)) AS "kunci!", id
        FROM categories
        WHERE lower(trim(name)) = ANY($1)
        "#,
        teks_list
    )
    .fetch_all(pool)
    .await?;
    for r in dari_nama {
        hasil.insert(r.kunci, r.id);
    }

    let belum: Vec<String> = teks_list
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !hasil.contains_key(t))
        .collect();

    if !belum.is_empty() {
        let dari_alias = sqlx::query_as!(
            Baris,
            r#"
            SELECT alias_norm AS "kunci!", target_id AS "id!"
            FROM import_aliases
            WHERE kind = 'category' AND alias_norm = ANY($1) AND target_id IS NOT NULL
            "#,
            &belum
        )
        .fetch_all(pool)
        .await?;
        for r in dari_alias {
            hasil.insert(r.kunci, r.id);
        }
    }

    Ok(hasil)
}

/// Mengajarkan pemetaan kategori baru: `alias_norm` -> `target_id`. Dipakai
/// lagi otomatis oleh `resolusi_kategori_banyak` pada unggahan berikutnya.
pub async fn ajarkan_alias(
    pool: &PgPool,
    alias_norm: &str,
    target_id: Uuid,
    created_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO import_aliases (kind, alias_norm, target_id, created_by, hits)
        VALUES ('category', $1, $2, $3, 1)
        ON CONFLICT (kind, alias_norm)
        DO UPDATE SET target_id = EXCLUDED.target_id, hits = import_aliases.hits + 1
        "#,
        alias_norm,
        target_id,
        created_by
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Menerapkan satu pemetaan kategori ke SELURUH baris batch ini yang
/// `category_text`-nya cocok (case-insensitive) -- dipanggil setelah
/// `ajarkan_alias`, supaya efeknya langsung terlihat di batch yang sedang
/// ditinjau, bukan cuma di unggahan berikutnya. WARN "kategori tidak
/// dikenal" pada baris yang kena juga dibuang dari `issues`.
pub async fn terapkan_kategori_ke_baris(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
    category_text: &str,
    category_id: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_rows
        SET category_id = $3,
            issues = COALESCE(
                (SELECT jsonb_agg(e) FROM jsonb_array_elements(issues) e
                 WHERE e->>'field' <> 'category_text'),
                '[]'::jsonb
            )
        WHERE batch_id = $1 AND lower(trim(category_text)) = lower(trim($2))
        "#,
        batch_id,
        category_text,
        category_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub struct ImportRowUntukCommit {
    pub id: Uuid,
    pub name: Option<String>,
    pub brand_text: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<String>,
    pub category_id: Option<Uuid>,
    pub lowest_price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub stock: Option<i32>,
    pub published: Option<bool>,
    pub action: String,
    pub match_product_id: Option<Uuid>,
}

/// Mengambil DAN mengunci satu baris yang masih perlu diproses --
/// `FOR UPDATE SKIP LOCKED` supaya kalaupun ada dua worker berjalan
/// bersamaan (lihat catatan desain di `worker.rs`), keduanya tidak pernah
/// memproses baris yang sama.
pub async fn next_pending_row(
    pool: &PgPool,
    batch_id: Uuid,
) -> AppResult<Option<ImportRowUntukCommit>> {
    let row = sqlx::query_as!(
        ImportRowUntukCommit,
        r#"
        SELECT id, name, brand_text, product_type, variant_grade, variant_size,
               category_id, lowest_price, cost, margin_pct, stock, published,
               action, match_product_id
        FROM import_rows
        WHERE batch_id = $1 AND commit_state = 'pending' AND action IN ('create', 'update')
        ORDER BY row_no
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#,
        batch_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Baris `action='skip'` tidak pernah diambil `next_pending_row`, tapi tetap
/// perlu ditandai `ok` supaya `hitung_hasil`/paginasi review konsisten --
/// dipanggil sekali di awal tiap putaran worker, lihat `worker.rs`.
pub async fn tandai_baris_dilewati_selesai(pool: &PgPool, batch_id: Uuid) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_rows SET commit_state = 'ok'
        WHERE batch_id = $1 AND action = 'skip' AND commit_state = 'pending'
        "#,
        batch_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn catat_hasil_baris(
    pool: &PgPool,
    row_id: Uuid,
    commit_state: &str,
    commit_product_id: Option<Uuid>,
    commit_error: Option<&str>,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_rows
        SET commit_state = $2, commit_product_id = $3, commit_error = $4
        WHERE id = $1
        "#,
        row_id,
        commit_state,
        commit_product_id,
        commit_error
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn reset_failed_rows(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_rows SET commit_state = 'pending', commit_error = NULL
        WHERE batch_id = $1 AND commit_state = 'failed'
        "#,
        batch_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub struct HasilHitung {
    pub ok: i32,
    pub fail: i32,
}

pub async fn hitung_hasil(pool: &PgPool, batch_id: Uuid) -> AppResult<HasilHitung> {
    struct Baris {
        ok: i64,
        fail: i64,
    }
    let baris = sqlx::query_as!(
        Baris,
        r#"
        SELECT
            count(*) FILTER (WHERE commit_state = 'ok')     AS "ok!",
            count(*) FILTER (WHERE commit_state = 'failed') AS "fail!"
        FROM import_rows
        WHERE batch_id = $1
        "#,
        batch_id
    )
    .fetch_one(pool)
    .await?;

    Ok(HasilHitung {
        ok: baris.ok as i32,
        fail: baris.fail as i32,
    })
}

pub async fn selesaikan_batch(pool: &PgPool, batch_id: Uuid, ok: i32, fail: i32) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE import_batches
        SET status = 'committed', committed_at = now(), ok_count = $2, fail_count = $3
        WHERE id = $1
        "#,
        batch_id,
        ok,
        fail
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Sama bentuknya dengan `ImportBatchTerkunci`, tapi TANPA `FOR UPDATE` --
/// dipakai worker membaca `publish_on_commit`/`uploaded_by` di LUAR
/// transaksi, karena pemrosesan tiap baris sengaja tidak menahan kunci
/// batch (lihat catatan desain di `worker.rs`).
pub async fn baca_batch_ringkas(pool: &PgPool, id: Uuid) -> AppResult<Option<ImportBatchTerkunci>> {
    let row = sqlx::query_as!(
        ImportBatchTerkunci,
        r#"SELECT status, publish_on_commit, uploaded_by, fail_count FROM import_batches WHERE id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Mengambil SATU batch berstatus `committing` untuk diproses, terkunci
/// `FOR UPDATE SKIP LOCKED` lalu segera dilepas (transaksi pendek) --
/// pemrosesan baris sesungguhnya terjadi di LUAR kunci ini, lihat catatan
/// desain di `worker.rs`.
pub async fn klaim_batch_committing(pool: &PgPool) -> AppResult<Option<Uuid>> {
    let mut tx = pool.begin().await?;

    let id = sqlx::query_scalar!(
        r#"
        SELECT id FROM import_batches
        WHERE status = 'committing'
        ORDER BY submitted_at NULLS LAST, uploaded_at
        LIMIT 1
        FOR UPDATE SKIP LOCKED
        "#
    )
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(id)
}
