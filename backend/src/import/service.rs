//! Alur kerja impor: unggah, tinjau, setujui, batalkan, ulangi, dan commit
//! satu baris (dipanggil worker).

use super::repo::{self, ImportBatchTerkunci, ImportRowUntukCommit, NewImportRow};
use super::{parse, reader, ImportStatus};
use crate::catalog::service::{self as catalog_service, CreateProductInput, ProductPatch};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::body::Bytes;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub async fn upload(
    state: &AppState,
    uploaded_by: Uuid,
    file_name: String,
    bytes: Bytes,
) -> AppResult<Uuid> {
    if bytes.len() > parse::BATAS_BYTE {
        return Err(AppError::bad_request(format!(
            "Ukuran berkas melebihi batas {} MB.",
            parse::BATAS_BYTE / (1024 * 1024)
        )));
    }

    let ekstensi = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    if !parse::EKSTENSI_DIIZINKAN.contains(&ekstensi.as_str()) {
        return Err(AppError::bad_request("Berkas harus .xlsx atau .csv."));
    }

    let (header, baris_mentah) = if ekstensi == "xlsx" {
        reader::baca_xlsx(&bytes)?
    } else {
        reader::baca_csv(&bytes)?
    };

    if baris_mentah.len() > parse::BATAS_BARIS {
        return Err(AppError::bad_request(format!(
            "Berkas berisi {} baris, melebihi batas {}.",
            baris_mentah.len(),
            parse::BATAS_BARIS
        )));
    }
    if baris_mentah.is_empty() {
        return Err(AppError::bad_request("Berkas tidak berisi baris data."));
    }

    let peta_kolom: Vec<Option<parse::Field>> =
        header.iter().map(|h| parse::map_header(h)).collect();

    let mut baris_terurai: Vec<parse::ParsedRow> = Vec::with_capacity(baris_mentah.len());
    let mut raw_json: Vec<serde_json::Value> = Vec::with_capacity(baris_mentah.len());

    for baris in &baris_mentah {
        let mut row = parse::ParsedRow::default();
        let mut raw_obj = serde_json::Map::new();

        for (i, sel) in baris.iter().enumerate() {
            if let Some(h) = header.get(i) {
                raw_obj.insert(h.clone(), serde_json::Value::String(sel.clone()));
            }
            let Some(field) = peta_kolom.get(i).copied().flatten() else {
                continue;
            };
            let nilai = sel.trim();
            if nilai.is_empty() {
                continue;
            }
            match field {
                parse::Field::Name => row.name = Some(nilai.to_string()),
                parse::Field::Sku => row.sku = Some(nilai.to_string()),
                parse::Field::Category => row.category_text = Some(nilai.to_string()),
                parse::Field::Brand => row.brand_text = Some(nilai.to_string()),
                parse::Field::ProductType => row.product_type = Some(nilai.to_string()),
                parse::Field::VariantGrade => row.variant_grade = Some(nilai.to_string()),
                parse::Field::VariantSize => row.variant_size = Some(nilai.to_string()),
                parse::Field::LowestPrice => row.lowest_price = parse::parse_money(nilai),
                parse::Field::Cost => row.cost = parse::parse_money(nilai),
                parse::Field::Margin => row.margin_pct = parse::parse_percent(nilai),
                parse::Field::Stock => row.stock = parse::parse_int(nilai),
                parse::Field::Published => row.published = parse::parse_bool(nilai),
            }
        }

        baris_terurai.push(row);
        raw_json.push(serde_json::Value::Object(raw_obj));
    }

    // SKU dobel DALAM BERKAS -- baris KEDUA yang ditandai, trim+lowercase
    // (BUKAN normalize_key, supaya "INV-001" dan "INV_001" tidak dianggap
    // sama -- lihat parse::validate_row).
    let mut sku_terlihat: HashSet<String> = HashSet::new();
    let sku_duplikat: Vec<bool> = baris_terurai
        .iter()
        .map(|r| match &r.sku {
            Some(s) => !sku_terlihat.insert(s.trim().to_lowercase()),
            None => false,
        })
        .collect();

    // SATU query untuk seluruh SKU unik -- bukan satu per baris.
    let sku_list: Vec<String> = baris_terurai
        .iter()
        .filter_map(|r| r.sku.as_deref())
        .map(|s| s.trim().to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let peta_sku = match repo::cari_produk_by_sku(&state.pool, &sku_list).await {
        Ok(peta) => peta,
        Err(err) => {
            // Lookup gagal TIDAK membatalkan unggahan -- semua baris
            // diperlakukan sebagai baru. Peringatannya sudah terlihat di
            // halaman review lewat WARN "kategori" yang serupa; di sini
            // cukup dicatat ke log.
            tracing::warn!(error = %err, "lookup SKU gagal saat unggah impor");
            HashMap::new()
        }
    };

    let kategori_list: Vec<String> = baris_terurai
        .iter()
        .filter_map(|r| r.category_text.as_deref())
        .map(|s| s.trim().to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let peta_kategori = repo::resolusi_kategori_banyak(&state.pool, &kategori_list).await?;

    let mut baris_baru = Vec::with_capacity(baris_terurai.len());
    for (i, row) in baris_terurai.iter().enumerate() {
        let match_product_id = row
            .sku
            .as_deref()
            .and_then(|s| peta_sku.get(&s.trim().to_lowercase()))
            .copied();
        let action = if match_product_id.is_some() {
            "update"
        } else {
            "create"
        };
        let category_id = row
            .category_text
            .as_deref()
            .and_then(|s| peta_kategori.get(&s.trim().to_lowercase()))
            .copied();
        let kategori_dikenal = row.category_text.is_none() || category_id.is_some();

        let issues = parse::validate_row(row, sku_duplikat[i], kategori_dikenal);
        let issues_json = serde_json::to_value(&issues).unwrap_or_else(|_| serde_json::json!([]));

        baris_baru.push(NewImportRow {
            row_no: (i + 1) as i32,
            raw: raw_json[i].clone(),
            name: row.name.clone(),
            sku: row.sku.clone(),
            category_text: row.category_text.clone(),
            brand_text: row.brand_text.clone(),
            product_type: row.product_type.clone(),
            variant_grade: row.variant_grade.clone(),
            variant_size: row.variant_size.clone(),
            category_id,
            lowest_price: row.lowest_price,
            cost: row.cost,
            margin_pct: row.margin_pct,
            stock: row.stock,
            published: row.published,
            action: action.to_string(),
            match_product_id,
            issues: issues_json,
        });
    }

    let file_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hex::encode(hasher.finalize())
    };

    let mut tx = state.pool.begin().await?;
    let batch_id = repo::insert_batch(
        &mut tx,
        &file_name,
        &file_sha256,
        baris_baru.len() as i32,
        uploaded_by,
    )
    .await?;
    repo::insert_rows(&mut tx, batch_id, &baris_baru).await?;
    tx.commit().await?;

    Ok(batch_id)
}

async fn ambil_terkunci(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    batch_id: Uuid,
) -> AppResult<ImportBatchTerkunci> {
    repo::kunci_batch(tx, batch_id)
        .await?
        .ok_or_else(|| AppError::not_found("Batch impor tidak ditemukan."))
}

pub async fn submit(
    pool: &PgPool,
    batch_id: Uuid,
    publish_on_commit: Option<bool>,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    ImportStatus::parse(&batch.status)?.pindah_ke(ImportStatus::PendingReview)?;

    if repo::ada_error_row(&mut tx, batch_id).await? {
        return Err(AppError::conflict(
            "Masih ada baris berstatus ERROR. Perbaiki berkasnya atau lewati barisnya.",
        ));
    }

    if let Some(nilai) = publish_on_commit {
        repo::set_publish_on_commit(&mut tx, batch_id, nilai).await?;
    }
    repo::tandai_disubmit(&mut tx, batch_id).await?;

    tx.commit().await?;
    Ok(())
}

/// Memvalidasi DUA langkah berurutan (`pending_review` -> `approved` ->
/// `committing`) tapi hanya `committing` yang tertulis -- worker langsung
/// mengambilnya begitu disetujui, jadi tidak ada gunanya menahan di
/// `approved` sesaat. Lihat catatan di `import::mod`.
pub async fn approve(
    pool: &PgPool,
    batch_id: Uuid,
    reviewed_by: Uuid,
    review_note: Option<&str>,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    let status = ImportStatus::parse(&batch.status)?;
    status.pindah_ke(ImportStatus::Approved)?;
    ImportStatus::Approved.pindah_ke(ImportStatus::Committing)?;

    if repo::ada_error_row(&mut tx, batch_id).await? {
        return Err(AppError::conflict(
            "Masih ada baris berstatus ERROR. Tidak bisa disetujui.",
        ));
    }

    repo::setujui_dan_mulai_commit(&mut tx, batch_id, reviewed_by, review_note).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn cancel(pool: &PgPool, batch_id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    let status = ImportStatus::parse(&batch.status)?;
    if !status.boleh_dibatalkan() {
        return Err(AppError::conflict(format!(
            "Batch berstatus \"{}\" tidak bisa dibatalkan.",
            status.as_str()
        )));
    }

    repo::set_status(&mut tx, batch_id, ImportStatus::Cancelled.as_str()).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn retry(pool: &PgPool, batch_id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    let status = ImportStatus::parse(&batch.status)?;
    if !status.boleh_diulang() {
        return Err(AppError::conflict(
            "Hanya batch yang sudah commit yang bisa diulang.",
        ));
    }
    if batch.fail_count == 0 {
        return Err(AppError::conflict("Tidak ada baris gagal untuk diulang."));
    }

    repo::reset_failed_rows(&mut tx, batch_id).await?;
    repo::set_status(&mut tx, batch_id, ImportStatus::Committing.as_str()).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn skip_row(pool: &PgPool, batch_id: Uuid, row_id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    let status = ImportStatus::parse(&batch.status)?;
    if !matches!(status, ImportStatus::Draft | ImportStatus::PendingReview) {
        return Err(AppError::conflict(
            "Baris hanya bisa dilewati saat batch masih draft atau menunggu tinjauan.",
        ));
    }

    if !repo::skip_row(&mut tx, batch_id, row_id).await? {
        return Err(AppError::not_found(
            "Baris tidak ditemukan atau sudah dilewati.",
        ));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn restore_row(pool: &PgPool, batch_id: Uuid, row_id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    let batch = ambil_terkunci(&mut tx, batch_id).await?;

    let status = ImportStatus::parse(&batch.status)?;
    if !matches!(status, ImportStatus::Draft | ImportStatus::PendingReview) {
        return Err(AppError::conflict(
            "Baris hanya bisa dikembalikan saat batch masih draft atau menunggu tinjauan.",
        ));
    }

    if !repo::restore_row(&mut tx, batch_id, row_id).await? {
        return Err(AppError::not_found(
            "Baris tidak ditemukan atau tidak sedang dilewati.",
        ));
    }

    tx.commit().await?;
    Ok(())
}

pub async fn map_category(
    pool: &PgPool,
    batch_id: Uuid,
    user_id: Uuid,
    category_text: &str,
    category_id: Uuid,
) -> AppResult<()> {
    let alias_norm = category_text.trim().to_lowercase();
    if alias_norm.is_empty() {
        return Err(AppError::bad_request("Teks kategori wajib diisi."));
    }

    repo::ajarkan_alias(pool, &alias_norm, category_id, user_id).await?;

    let mut tx = pool.begin().await?;
    repo::terapkan_kategori_ke_baris(&mut tx, batch_id, category_text, category_id).await?;
    tx.commit().await?;

    Ok(())
}

/// Commit SATU baris -- dipanggil worker (`super::worker`). Idempoten:
/// worker hanya mengambil baris `commit_state='pending'`, jadi baris yang
/// sudah `ok` tidak pernah lewat sini lagi.
pub async fn commit_row(
    state: &AppState,
    batch: &ImportBatchTerkunci,
    row: &ImportRowUntukCommit,
) -> (&'static str, Option<Uuid>, Option<String>) {
    match proses_baris(state, batch, row).await {
        Ok(id) => ("ok", Some(id), None),
        Err(err) => ("failed", None, Some(err.to_string())),
    }
}

async fn proses_baris(
    state: &AppState,
    batch: &ImportBatchTerkunci,
    row: &ImportRowUntukCommit,
) -> AppResult<Uuid> {
    let (harga, _) = parse::harga_efektif(row.lowest_price, row.cost, row.margin_pct);

    match row.action.as_str() {
        "create" => {
            let created_by = batch
                .uploaded_by
                .ok_or_else(|| AppError::internal("Batch impor tanpa pengunggah."))?;
            // Baris CREATE dengan harga kosong = 0 (sudah diberi WARN saat
            // upload, lihat parse::validate_row) -- BEDA dari baris UPDATE
            // di bawah, yang `None` berarti jangan sentuh harga sama sekali.
            let is_active = batch.publish_on_commit && row.published.unwrap_or(true);

            catalog_service::create_product(
                state,
                created_by,
                CreateProductInput {
                    name: row.name.clone().unwrap_or_default(),
                    seo_name: None,
                    brand_name: row.brand_text.clone(),
                    product_type: row.product_type.clone(),
                    variant_grade: row.variant_grade.clone(),
                    variant_size: row.variant_size.clone(),
                    parent_id: None,
                    category_id: row.category_id,
                    price: harga.unwrap_or(Decimal::ZERO),
                    price_shopee: None,
                    price_tiktok: None,
                    cost_price: row.cost,
                    is_active,
                    low_stock_threshold: None,
                    image_url: None,
                    stock_qty: row.stock.unwrap_or(0).max(0),
                    purchase_price: row.cost,
                    batch_number: None,
                    expiry_date: None,
                    storage_location: None,
                },
            )
            .await
        }
        "update" => {
            let target = row
                .match_product_id
                .ok_or_else(|| AppError::internal("Baris update tanpa match_product_id."))?;

            // Toggle batch menentukan apakah `published` DISENTUH sama
            // sekali -- bukan nilainya. Kalau toggle mati, kolom `published`
            // baris (kosong ataupun terisi) tidak pernah mengubah is_active
            // produk yang sudah ada.
            let is_active = if batch.publish_on_commit {
                row.published
            } else {
                None
            };

            catalog_service::update_product(
                state,
                target,
                ProductPatch {
                    // `harga` dari rumus/eksplisit; `None` (harga & modal &
                    // margin semua kosong) berarti JANGAN UBAH harga --
                    // beda dari baris create yang jatuh ke 0.
                    price: harga,
                    cost_price: row.cost.map(Some),
                    category_id: row.category_id.map(Some),
                    is_active,
                    ..Default::default()
                },
            )
            .await?;

            Ok(target)
        }
        lain => Err(AppError::internal(format!(
            "Aksi baris \"{lain}\" tidak dikenal."
        ))),
    }
}
