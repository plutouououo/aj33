//! Endpoint tiket packing.

use super::repo::{self, Ticket};
use super::service;
use super::TicketStatus;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders/{id}/ticket", post(create_ticket))
        .route("/tickets", get(list_tickets))
        .route("/tickets/{id}", get(get_ticket))
        .route("/tickets/{id}/assign", post(assign))
        .route("/tickets/{id}/status", patch(set_status))
        .route("/tickets/{id}/items/{item_id}", patch(set_item))
}

#[derive(Debug, Deserialize)]
struct CreateTicketRequest {
    /// Catatan untuk pengepak, mis. "bungkus bubble wrap dobel".
    #[serde(default)]
    notes: Option<String>,
}

async fn create_ticket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(order_id): Path<Uuid>,
    body: Option<Json<CreateTicketRequest>>,
) -> AppResult<(axum::http::StatusCode, Json<Ticket>)> {
    user.require(&[Role::Owner])?;

    let notes = body
        .and_then(|Json(b)| b.notes)
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());

    let id = service::buat(&state.pool, order_id, notes.as_deref()).await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(muat(&state, id).await?),
    ))
}

#[derive(Debug, Deserialize)]
struct TicketListQuery {
    status: Option<String>,
    /// `mine=true` menyaring ke tiket milik sendiri -- inilah tampilan
    /// utama layar pengepak.
    #[serde(default)]
    mine: bool,
}

async fn list_tickets(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<TicketListQuery>,
) -> AppResult<Json<Vec<Ticket>>> {
    user.require(&[Role::Owner, Role::Pengepak])?;

    let status = match q.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // Divalidasi lebih dulu supaya status yang salah tulis menghasilkan
        // pesan jelas, bukan daftar kosong yang membingungkan.
        Some(raw) => Some(TicketStatus::parse(raw)?.as_str()),
        None => None,
    };

    let rows = repo::list(
        &state.pool,
        status,
        if q.mine { Some(user.id) } else { None },
    )
    .await?;

    Ok(Json(rows))
}

/// Pengepak boleh membaca tiket mana pun, termasuk yang belum ditugaskan --
/// dari situlah dia memutuskan mau mengambil yang mana. Yang dibatasi
/// pemiliknya adalah mengubah tiket, bukan melihatnya.
async fn get_ticket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Ticket>> {
    user.require(&[Role::Owner, Role::Pengepak])?;
    Ok(Json(muat(&state, id).await?))
}

#[derive(Debug, Deserialize)]
struct AssignRequest {
    /// Kosong berarti "ambil untuk diri sendiri".
    #[serde(default)]
    user_id: Option<Uuid>,
}

async fn assign(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<AssignRequest>>,
) -> AppResult<Json<Ticket>> {
    user.require(&[Role::Owner, Role::Pengepak])?;

    let kepada = body.and_then(|Json(b)| b.user_id).unwrap_or(user.id);
    service::tugaskan(&state.pool, id, kepada, user).await?;

    Ok(Json(muat(&state, id).await?))
}

#[derive(Debug, Deserialize)]
struct StatusRequest {
    status: String,
}

async fn set_status(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusRequest>,
) -> AppResult<Json<Ticket>> {
    user.require(&[Role::Owner, Role::Pengepak])?;

    let tujuan = TicketStatus::parse(body.status.trim())
        .map_err(|_| AppError::bad_request("Status tiket tidak dikenal."))?;

    service::pindah_status(&state.pool, id, tujuan, user).await?;

    Ok(Json(muat(&state, id).await?))
}

#[derive(Debug, Deserialize)]
struct ItemRequest {
    is_packed: bool,
}

async fn set_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, item_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ItemRequest>,
) -> AppResult<Json<Ticket>> {
    user.require(&[Role::Owner, Role::Pengepak])?;

    service::tandai_item(&state.pool, id, item_id, body.is_packed, user).await?;

    Ok(Json(muat(&state, id).await?))
}

/// Tiap perubahan menjawab dengan tiket utuh yang baru.
///
/// Layar packing dipakai sambil memegang barang: satu jawaban yang sudah
/// berisi keadaan terbaru menghemat satu bolak-balik jaringan, dan yang
/// lebih penting, menghilangkan kemungkinan layar menampilkan keadaan lama
/// karena permintaan kedua gagal.
async fn muat(state: &AppState, id: Uuid) -> AppResult<Ticket> {
    repo::find(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Tiket tidak ditemukan."))
}
