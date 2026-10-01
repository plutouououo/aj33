//! Endpoint pengaturan harga grosir.

use super::repo;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new().route("/settings/pricing", get(get_pricing).patch(update_pricing))
}

/// Batas atas mengikuti NUMERIC(8,3) di kolomnya; lebih dari itu galat database yang tak terbaca pemilik.
const BATAS_MAKS_KG: Decimal = Decimal::from_parts(99_999_999, 0, 0, false, 3);

#[derive(Debug, Serialize)]
struct PricingSettings {
    /// Baris kanal toko memakai harga grosir bila berat baris (qty x ukuran pack) LEBIH dari angka ini.
    wholesale_threshold_kg: Decimal,
}

/// Dibaca semua peran login karena kasir memakainya menampilkan harga baris yang sama dengan yang ditagih backend.
async fn get_pricing(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<PricingSettings>> {
    Ok(Json(PricingSettings {
        wholesale_threshold_kg: repo::batas_grosir_kg(&state.pool).await?,
    }))
}

#[derive(Debug, Deserialize)]
struct PricingUpdateRequest {
    wholesale_threshold_kg: Decimal,
}

async fn update_pricing(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<PricingUpdateRequest>,
) -> AppResult<Json<PricingSettings>> {
    user.require(&[Role::Owner])?;

    let batas = body.wholesale_threshold_kg;
    if batas <= Decimal::ZERO || batas > BATAS_MAKS_KG {
        return Err(AppError::bad_request(
            "Batas grosir harus lebih dari 0 dan paling banyak 99.999,999 kg.",
        ));
    }

    repo::ubah_batas_grosir_kg(&state.pool, batas, user.id).await?;

    Ok(Json(PricingSettings {
        wholesale_threshold_kg: batas,
    }))
}
