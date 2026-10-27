//! Endpoint edit massal produk; semuanya khusus Owner.

use super::rencana::{self, Lembar};
use super::repo as sesi_repo;
use super::{ekspor, terapkan, Baris, HasilBaris, Kelompok, StatusBaris};
use crate::auth::{CurrentUser, Role};
use crate::catalog::repo::{self as katalog, Product, ProductFilter, Scope};
use crate::error::{AppError, AppResult};
use crate::import::parse::BATAS_BYTE;
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

const TIPE_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bulk-edit/export", get(unduh))
        .route(
            "/bulk-edit",
            post(unggah).layer(DefaultBodyLimit::max(BATAS_BYTE)),
        )
        .route("/bulk-edit/{id}", get(detail))
        .route("/bulk-edit/{id}/apply", post(terapkan_sesi))
}

/// Seluruh produk (induk dan varian) menurut saringan daftar; tanpa saringan berarti seluruh katalog.
async fn semua_produk(
    pool: &PgPool,
    cari: Option<String>,
    kategori: Option<Uuid>,
) -> AppResult<Vec<Product>> {
    let daftar = katalog::list_products(
        pool,
        &ProductFilter {
            search: cari,
            category_id: kategori,
            parent_id: None,
            scope: Scope::Semua,
            only_active: false,
            limit: 100_000,
            offset: 0,
        },
    )
    .await?;
    Ok(daftar.rows)
}

#[derive(Debug, Deserialize)]
struct UnduhQuery {
    kelompok: Kelompok,
    cari: Option<String>,
    kategori: Option<Uuid>,
}

async fn unduh(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<UnduhQuery>,
) -> AppResult<impl IntoResponse> {
    user.require(&[Role::Owner])?;

    let cari = q
        .cari
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());
    let produk = semua_produk(&state.pool, cari, q.kategori).await?;

    let (isi, nama) = match q.kelompok {
        Kelompok::Utama => (ekspor::utama(&produk)?, "edit-massal-utama.xlsx"),
        Kelompok::Stok => {
            let ids: HashSet<Uuid> = produk.iter().map(|p| p.id).collect();
            let batch: Vec<_> = sesi_repo::batch_semua(&state.pool)
                .await?
                .into_iter()
                .filter(|b| ids.contains(&b.product_id) && b.remaining_qty > 0)
                .collect();
            (ekspor::stok(&produk, &batch)?, "edit-massal-stok.xlsx")
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, TIPE_XLSX.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{nama}\""),
            ),
        ],
        isi,
    ))
}

#[derive(Debug, Deserialize)]
struct UnggahQuery {
    kelompok: Kelompok,
    filename: String,
}

#[derive(Debug, Serialize)]
struct Dibuat {
    id: Uuid,
}

/// `?filename=` lewat query seperti `import::routes::upload`, menghindari batasan ASCII header.
async fn unggah(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<UnggahQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<Dibuat>)> {
    user.require(&[Role::Owner])?;

    if !q.filename.to_lowercase().ends_with(".xlsx") {
        return Err(AppError::bad_request("Berkas harus berekstensi .xlsx."));
    }
    if body.is_empty() {
        return Err(AppError::bad_request("Berkas kosong."));
    }

    let lembar = Lembar::baca(&body)?;
    let produk = semua_produk(&state.pool, None, None).await?;
    let baris = match q.kelompok {
        Kelompok::Utama => {
            let kategori = katalog::list_categories(&state.pool).await?;
            rencana::rencana_utama(&lembar, &produk, &kategori)?
        }
        Kelompok::Stok => {
            let batch = sesi_repo::batch_semua(&state.pool).await?;
            rencana::rencana_stok(&lembar, &produk, &batch)?
        }
    };
    if baris.is_empty() {
        return Err(AppError::bad_request("Berkas tidak berisi baris data."));
    }

    let json = serde_json::to_value(&baris).map_err(|err| {
        AppError::internal(format!("Rencana edit massal gagal diserialkan: {err}"))
    })?;
    let id = sesi_repo::simpan_sesi(
        &state.pool,
        q.kelompok.as_str(),
        &q.filename,
        &json,
        user.id,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(Dibuat { id })))
}

#[derive(Debug, Serialize)]
struct Ringkasan {
    baru: usize,
    edit: usize,
    tetap: usize,
    error: usize,
}

#[derive(Debug, Serialize)]
struct Detail {
    id: Uuid,
    kelompok: String,
    file_name: String,
    status: String,
    created_at: DateTime<Utc>,
    applied_at: Option<DateTime<Utc>>,
    ringkasan: Ringkasan,
    baris: Vec<Baris>,
    hasil: Option<Vec<HasilBaris>>,
}

fn susun_detail(sesi: sesi_repo::Sesi) -> AppResult<Detail> {
    let baris: Vec<Baris> = serde_json::from_value(sesi.baris)
        .map_err(|err| AppError::internal(format!("Rencana edit massal rusak: {err}")))?;
    let hasil: Option<Vec<HasilBaris>> = sesi
        .hasil
        .map(serde_json::from_value)
        .transpose()
        .map_err(|err| AppError::internal(format!("Hasil edit massal rusak: {err}")))?;

    let hitung = |status: StatusBaris| baris.iter().filter(|b| b.status == status).count();
    let ringkasan = Ringkasan {
        baru: hitung(StatusBaris::Baru),
        edit: hitung(StatusBaris::Edit),
        tetap: hitung(StatusBaris::Tetap),
        error: hitung(StatusBaris::Error),
    };

    Ok(Detail {
        id: sesi.id,
        kelompok: sesi.kelompok,
        file_name: sesi.file_name,
        status: sesi.status,
        created_at: sesi.created_at,
        applied_at: sesi.applied_at,
        ringkasan,
        baris,
        hasil,
    })
}

async fn ambil_detail(pool: &PgPool, id: Uuid) -> AppResult<Detail> {
    let sesi = sesi_repo::ambil_sesi(pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Pratinjau edit massal tidak ditemukan."))?;
    susun_detail(sesi)
}

async fn detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Detail>> {
    user.require(&[Role::Owner])?;
    Ok(Json(ambil_detail(&state.pool, id).await?))
}

async fn terapkan_sesi(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Detail>> {
    user.require(&[Role::Owner])?;

    let Some((_, baris_json)) = sesi_repo::klaim_sesi(&state.pool, id).await? else {
        // Baris tak terklaim: tak pernah ada, atau sudah diterapkan sebelumnya.
        ambil_detail(&state.pool, id).await?;
        return Err(AppError::conflict("Pratinjau ini sudah diterapkan."));
    };
    let baris: Vec<Baris> = serde_json::from_value(baris_json)
        .map_err(|err| AppError::internal(format!("Rencana edit massal rusak: {err}")))?;

    let hasil = terapkan::jalankan(&state, user.id, &baris).await;
    let json = serde_json::to_value(&hasil)
        .map_err(|err| AppError::internal(format!("Hasil edit massal gagal diserialkan: {err}")))?;
    sesi_repo::simpan_hasil(&state.pool, id, &json).await?;

    Ok(Json(ambil_detail(&state.pool, id).await?))
}
