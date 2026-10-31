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
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders/{id}/ticket", post(create_ticket))
        .route("/tickets", get(list_tickets))
        .route("/tickets/bulk", post(kirim_tiket))
        .route("/tickets/{id}", get(get_ticket).delete(batalkan_tiket))
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
struct BatchPilihan {
    product_id: Uuid,
    batch_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct PesananPilihan {
    order_id: Uuid,
    #[serde(default)]
    batches: Vec<BatchPilihan>,
}

#[derive(Debug, Deserialize)]
struct KirimRequest {
    user_id: Uuid,
    orders: Vec<PesananPilihan>,
}

#[derive(Debug, Serialize)]
struct KirimResponse {
    ticket_ids: Vec<Uuid>,
}

async fn kirim_tiket(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<KirimRequest>,
) -> AppResult<(axum::http::StatusCode, Json<KirimResponse>)> {
    user.require(&[Role::Owner])?;

    let pesanan: Vec<service::PesananKirim> = body
        .orders
        .into_iter()
        .map(|p| service::PesananKirim {
            external_order_id: p.order_id,
            batches: p
                .batches
                .into_iter()
                .map(|b| (b.product_id, b.batch_id))
                .collect(),
        })
        .collect();

    let ticket_ids = service::kirim(&state.pool, &pesanan, body.user_id, user).await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(KirimResponse { ticket_ids }),
    ))
}

#[derive(Debug, Deserialize)]
struct TicketListQuery {
    status: Option<String>,
    /// `mine=true` menyaring ke tiket sendiri, tampilan utama layar pengepak.
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
        // Divalidasi dulu agar status salah tulis menghasilkan pesan jelas, bukan daftar kosong membingungkan.
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

/// Pengepak boleh membaca tiket mana pun (termasuk belum ditugaskan, untuk memilih yang diambil); yang dibatasi pemiliknya adalah mengubah, bukan melihat.
async fn get_ticket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Ticket>> {
    user.require(&[Role::Owner, Role::Pengepak])?;
    Ok(Json(muat(&state, id).await?))
}

async fn batalkan_tiket(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;
    service::batalkan(&state.pool, id, user).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
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
        .map_err(|_| AppError::bad_request("Status tugas packing tidak dikenal."))?;

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

/// Tiap perubahan menjawab tiket utuh baru: layar packing dipakai sambil memegang barang, jadi satu jawaban menghemat bolak-balik jaringan dan mencegah layar menampilkan keadaan lama.
async fn muat(state: &AppState, id: Uuid) -> AppResult<Ticket> {
    repo::find(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Tugas packing tidak ditemukan."))
}
