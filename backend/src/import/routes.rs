//! Endpoint impor produk massal.

use super::repo::{self, ImportBatch, ImportRow};
use super::service;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/imports", get(list_batches).post(upload))
        .route("/imports/{id}", get(get_batch))
        .route("/imports/{id}/rows", get(list_rows))
        .route("/imports/{id}/status", get(get_status))
        .route("/imports/{id}/map", post(map_category))
        .route("/imports/{id}/rows/{row_id}/skip", post(skip_row))
        .route("/imports/{id}/rows/{row_id}/restore", post(restore_row))
        .route("/imports/{id}/submit", post(submit))
        .route("/imports/{id}/approve", post(approve))
        .route("/imports/{id}/cancel", post(cancel))
        .route("/imports/{id}/retry", post(retry))
}

const LIMIT_MAKS: i64 = 200;

#[derive(Debug, Deserialize)]
struct ListQuery {
    page: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    filename: String,
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    batch_id: Uuid,
}

/// `?filename=` lewat query, bukan header, menghindari batasan ASCII nilai header untuk nama berspasi/beraksen; Astro membangunnya lewat `URLSearchParams`.
async fn upload(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    user.require(&[Role::Owner])?;

    let batch_id = service::upload(&state, user.id, q.filename, body).await?;
    Ok((StatusCode::CREATED, Json(UploadResponse { batch_id })))
}

#[derive(Debug, Serialize)]
struct PaginatedBatches {
    data: Vec<ImportBatch>,
    page: i64,
    limit: i64,
    total: i64,
}

async fn list_batches(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<PaginatedBatches>> {
    user.require(&[Role::Owner])?;

    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(20).clamp(1, LIMIT_MAKS);
    let (data, total) = repo::list_batches(&state.pool, limit, (page - 1) * limit).await?;

    Ok(Json(PaginatedBatches {
        data,
        page,
        limit,
        total,
    }))
}

#[derive(Debug, Serialize)]
struct ImportBatchDetail {
    #[serde(flatten)]
    batch: ImportBatch,
    create_count: i64,
    update_count: i64,
    skip_count: i64,
    error_count: i64,
    warn_count: i64,
}

async fn get_batch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ImportBatchDetail>> {
    user.require(&[Role::Owner])?;

    let batch = repo::find_batch(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Batch impor tidak ditemukan."))?;
    let ringkasan = repo::hitung_ringkasan(&state.pool, id).await?;

    Ok(Json(ImportBatchDetail {
        batch,
        create_count: ringkasan.create_count,
        update_count: ringkasan.update_count,
        skip_count: ringkasan.skip_count,
        error_count: ringkasan.error_count,
        warn_count: ringkasan.warn_count,
    }))
}

/// Ringan sengaja karena dipanggil halaman review tiap auto-refresh saat `status=committing`, tanpa memuat seluruh baris.
#[derive(Debug, Serialize)]
struct StatusResponse {
    status: String,
    ok_count: i32,
    fail_count: i32,
    total_rows: i32,
}

async fn get_status(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<StatusResponse>> {
    user.require(&[Role::Owner])?;

    let batch = repo::find_batch(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Batch impor tidak ditemukan."))?;

    Ok(Json(StatusResponse {
        status: batch.status,
        ok_count: batch.ok_count,
        fail_count: batch.fail_count,
        total_rows: batch.total_rows,
    }))
}

#[derive(Debug, Serialize)]
struct PaginatedRows {
    data: Vec<ImportRow>,
    page: i64,
    limit: i64,
    total: i64,
}

async fn list_rows(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<PaginatedRows>> {
    user.require(&[Role::Owner])?;

    let page = q.page.unwrap_or(1).max(1);
    let limit = q.limit.unwrap_or(50).clamp(1, LIMIT_MAKS);
    let (data, total) = repo::list_rows(&state.pool, id, limit, (page - 1) * limit).await?;

    Ok(Json(PaginatedRows {
        data,
        page,
        limit,
        total,
    }))
}

#[derive(Debug, Deserialize)]
struct MapCategoryRequest {
    category_text: String,
    category_id: Uuid,
}

async fn map_category(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MapCategoryRequest>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::map_category(
        &state.pool,
        id,
        user.id,
        &body.category_text,
        body.category_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn skip_row(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, row_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::skip_row(&state.pool, id, row_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_row(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, row_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::restore_row(&state.pool, id, row_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct SubmitRequest {
    publish_on_commit: Option<bool>,
}

async fn submit(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitRequest>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::submit(&state.pool, id, body.publish_on_commit).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ApproveRequest {
    review_note: Option<String>,
}

async fn approve(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ApproveRequest>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::approve(&state.pool, id, user.id, body.review_note.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::cancel(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn retry(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    service::retry(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
