//! Endpoint langganan Web Push.

use super::repo::{self, Langganan};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/push-subscriptions", post(simpan).delete(hapus))
        .route("/users/{id}/push-subscriptions", get(milik))
}

#[derive(Debug, Deserialize)]
struct Kunci {
    p256dh: String,
    auth: String,
}

/// Bentuk sama dengan `PushSubscription.toJSON()` di browser agar Astro bisa meneruskannya apa adanya.
#[derive(Debug, Deserialize)]
struct SimpanRequest {
    endpoint: String,
    keys: Kunci,
}

async fn simpan(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<SimpanRequest>,
) -> AppResult<StatusCode> {
    let endpoint = body.endpoint.trim();
    if !endpoint.starts_with("https://") || endpoint.len() > 2000 {
        return Err(AppError::bad_request(
            "Alamat langganan notifikasi tidak sah.",
        ));
    }
    if body.keys.p256dh.is_empty()
        || body.keys.auth.is_empty()
        || body.keys.p256dh.len() > 200
        || body.keys.auth.len() > 100
    {
        return Err(AppError::bad_request(
            "Kunci langganan notifikasi tidak sah.",
        ));
    }

    repo::simpan(
        &state.pool,
        user.id,
        &Langganan {
            endpoint: endpoint.to_string(),
            p256dh: body.keys.p256dh,
            auth: body.keys.auth,
        },
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct HapusRequest {
    endpoint: String,
}

async fn hapus(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<HapusRequest>,
) -> AppResult<StatusCode> {
    let pemilik = if user.role == Role::Owner {
        None
    } else {
        Some(user.id)
    };
    repo::hapus(&state.pool, body.endpoint.trim(), pemilik).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Owner saja: dipanggil server Astro saat owner mengirim tiket, untuk mengirim notifikasi ke perangkat pengepak tujuan.
async fn milik(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<Langganan>>> {
    user.require(&[Role::Owner])?;
    Ok(Json(repo::milik(&state.pool, id).await?))
}
