//! Endpoint transaksi POS.

use super::repo::{self, Transaction};
use super::service::{
    self, CheckoutInput, CheckoutItem, PaymentMethod, SalesChannel, TransactionType,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transactions", get(list_transactions).post(checkout))
        .route("/transactions/{id}", get(get_transaction))
        .route("/transactions/{id}/void", post(void_transaction))
}

#[derive(Debug, Deserialize)]
struct CheckoutRequest {
    #[serde(rename = "type", default = "default_type")]
    transaction_type: TransactionType,
    customer_id: Option<Uuid>,
    payment_method: PaymentMethod,
    /// Daftar harga yang dipakai; kosong berarti harga toko (pembeli di depan meja, satu-satunya kanal sebelum migrasi 0015).
    #[serde(default = "default_channel")]
    sales_channel: SalesChannel,
    amount_paid: Option<Decimal>,
    /// Ongkos kirim. Kosong berarti nol, bukan "tidak diketahui".
    shipping_cost: Option<Decimal>,
    /// Potongan atas seluruh belanja. Kosong berarti nol.
    discount_amount: Option<Decimal>,
    /// Persentase `commission_fee` Shopee sebagai pecahan (0,1725 = 17,25%), hanya untuk Shopee; kosong jatuh ke `shopee_commission_persen_default`.
    platform_commission_fee_percent: Option<Decimal>,
    /// Persentase `service_fee` Shopee (program opsional), kosong berarti nol (`shopee_service_persen_default`).
    platform_service_fee_percent: Option<Decimal>,
    items: Vec<CheckoutItem>,
}

fn default_type() -> TransactionType {
    TransactionType::WalkIn
}

fn default_channel() -> SalesChannel {
    SalesChannel::Toko
}

async fn checkout(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(body): Json<CheckoutRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Transaction>)> {
    user.require(&[Role::Kasir, Role::Owner])?;

    // Header wajib karena tanpa kunci request yang terkirim ulang karena jaringan putus jadi transaksi kedua (stok dua kali, tagihan dua kali).
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("Header Idempotency-Key wajib diisi."))?;

    if idempotency_key.len() > 100 {
        return Err(AppError::bad_request(
            "Idempotency-Key terlalu panjang (maksimal 100 karakter).",
        ));
    }

    let transaksi = service::checkout(
        &state.pool,
        CheckoutInput {
            idempotency_key: idempotency_key.to_string(),
            transaction_type: body.transaction_type,
            customer_id: body.customer_id,
            payment_method: body.payment_method,
            sales_channel: body.sales_channel,
            amount_paid: body.amount_paid,
            shipping_cost: body.shipping_cost,
            discount_amount: body.discount_amount,
            platform_commission_fee_percent: body.platform_commission_fee_percent,
            platform_service_fee_percent: body.platform_service_fee_percent,
            items: body.items,
            cashier_user_id: user.id,
        },
    )
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(transaksi)))
}

async fn get_transaction(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Transaction>> {
    let transaksi = repo::find_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Transaksi tidak ditemukan."))?;

    Ok(Json(transaksi))
}

async fn list_transactions(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<Transaction>>> {
    let rows = repo::list_transactions(&state.pool, 100).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct VoidRequest {
    reason: String,
}

async fn void_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VoidRequest>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;

    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(AppError::bad_request("Alasan pembatalan wajib diisi."));
    }

    service::void_transaction(&state.pool, id, user.id, reason).await?;
    Ok(StatusCode::NO_CONTENT)
}
