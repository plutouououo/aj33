//! Endpoint pelanggan.

use super::repo::{self, Customer, CustomerFilter};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new().route("/customers", get(list_customers).post(create_customer))
}

/// Batas atas supaya satu request tidak bisa menarik seluruh tabel.
const LIMIT_MAKS: i64 = 200;

/// Panjang kolom di skema awal. Diperiksa di sini supaya nama yang
/// kepanjangan dijawab 400 dengan pesan yang bisa dibaca kasir, bukan 500
/// dari Postgres.
const NAMA_MAKS: usize = 150;
const TELEPON_MAKS: usize = 30;

#[derive(Debug, Deserialize)]
struct ListQuery {
    search: Option<String>,
    limit: Option<i64>,
}

/// Terbuka untuk semua peran yang sudah login: kasir membacanya saat
/// checkout, dan halaman pesanan nanti membacanya juga.
async fn list_customers(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Customer>>> {
    let filter = CustomerFilter {
        search: q
            .search
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        limit: q.limit.unwrap_or(50).clamp(1, LIMIT_MAKS),
    };

    Ok(Json(repo::list_customers(&state.pool, &filter).await?))
}

#[derive(Debug, Deserialize)]
struct CustomerCreateRequest {
    name: String,
    phone: Option<String>,
}

async fn create_customer(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CustomerCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Customer>)> {
    user.require(&[Role::Kasir, Role::Owner])?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("Nama pelanggan wajib diisi."));
    }
    if name.chars().count() > NAMA_MAKS {
        return Err(AppError::bad_request(format!(
            "Nama pelanggan terlalu panjang (maksimal {NAMA_MAKS} karakter)."
        )));
    }

    let phone = body
        .phone
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(phone) = phone {
        if phone.chars().count() > TELEPON_MAKS {
            return Err(AppError::bad_request(format!(
                "Nomor telepon terlalu panjang (maksimal {TELEPON_MAKS} karakter)."
            )));
        }

        // Pelanggan langganan yang kembali tidak perlu jadi baris baru tiap
        // kali kasir mengetikkan namanya lagi. Dijawab 200, bukan 201:
        // tidak ada yang dibuat.
        if let Some(lama) = repo::find_by_phone(&state.pool, phone).await? {
            return Ok((axum::http::StatusCode::OK, Json(lama)));
        }
    }

    let customer = repo::insert_customer(&state.pool, name, phone).await?;
    Ok((axum::http::StatusCode::CREATED, Json(customer)))
}
