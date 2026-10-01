//! Endpoint pengaturan harga: batas grosir dan persen biaya Shopee.

use super::repo;
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::routing::{get, patch};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings/pricing", get(get_pricing).patch(update_pricing))
        .route("/settings/shopee-fees", patch(update_shopee_fees))
}

/// Batas atas mengikuti NUMERIC(8,3) di kolomnya; lebih dari itu galat database yang tak terbaca pemilik.
const BATAS_MAKS_KG: Decimal = Decimal::from_parts(99_999_999, 0, 0, false, 3);

#[derive(Debug, Serialize)]
struct PricingSettings {
    /// Baris kanal toko memakai harga grosir bila berat baris (qty x ukuran pack) LEBIH dari angka ini.
    wholesale_threshold_kg: Decimal,
    /// Pecahan (0,1725 = 17,25%) komisi Shopee yang dipakai checkout bila kasir tak menyebut persen sendiri.
    shopee_commission_percent: Decimal,
    /// Pecahan biaya layanan Shopee (program opsional), bawaan nol.
    shopee_service_percent: Decimal,
}

async fn baca_pricing(state: &AppState) -> AppResult<PricingSettings> {
    let biaya = repo::biaya_shopee(&state.pool).await?;
    Ok(PricingSettings {
        wholesale_threshold_kg: repo::batas_grosir_kg(&state.pool).await?,
        shopee_commission_percent: biaya.komisi,
        shopee_service_percent: biaya.layanan,
    })
}

/// Dibaca semua peran login karena kasir memakainya menampilkan harga baris yang sama dengan yang ditagih backend.
async fn get_pricing(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<PricingSettings>> {
    Ok(Json(baca_pricing(&state).await?))
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

    Ok(Json(baca_pricing(&state).await?))
}

#[derive(Debug, Deserialize)]
struct ShopeeFeesUpdateRequest {
    shopee_commission_percent: Decimal,
    shopee_service_percent: Decimal,
}

async fn update_shopee_fees(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ShopeeFeesUpdateRequest>,
) -> AppResult<Json<PricingSettings>> {
    user.require(&[Role::Owner])?;

    for (persen, label) in [
        (body.shopee_commission_percent, "komisi"),
        (body.shopee_service_percent, "layanan"),
    ] {
        if persen.is_sign_negative() || persen > Decimal::ONE {
            return Err(AppError::bad_request(format!(
                "Persentase biaya {label} harus di antara 0% dan 100%."
            )));
        }
    }

    // Kolomnya NUMERIC(6,4): pecahan dengan lebih dari 4 desimal (mis. 17,255%) dibulatkan database tanpa kabar, jadi ditolak di sini.
    if body.shopee_commission_percent.normalize().scale() > 4
        || body.shopee_service_percent.normalize().scale() > 4
    {
        return Err(AppError::bad_request(
            "Persentase biaya Shopee paling banyak 2 angka di belakang koma.",
        ));
    }

    repo::ubah_biaya_shopee(
        &state.pool,
        &repo::BiayaShopee {
            komisi: body.shopee_commission_percent,
            layanan: body.shopee_service_percent,
        },
        user.id,
    )
    .await?;

    Ok(Json(baca_pricing(&state).await?))
}
