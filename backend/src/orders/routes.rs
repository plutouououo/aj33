//! Endpoint platform, webhook, pesanan marketplace, dan pemetaan produk.

use super::repo::{self, Mapping, Order, PlatformStatus};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::marketplace::tiktok::{self, NAMA_PLATFORM};
use crate::marketplace::{klasifikasi_sla, tenggat_sla};
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/platforms", get(list_platforms))
        .route("/platforms/tiktok/connect", get(connect))
        .route("/platforms/tiktok/callback", get(callback))
        .route("/platforms/tiktok/disconnect", post(disconnect))
        .route("/webhooks/tiktok", post(webhook))
        .route("/orders", get(list_orders))
        .route("/orders/{id}", get(get_order))
        .route(
            "/products/{id}/mappings",
            get(list_mappings).post(create_mapping),
        )
        .route(
            "/products/{id}/mappings/{mapping_id}",
            axum::routing::delete(delete_mapping),
        )
}

// ---------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PlatformDto {
    #[serde(flatten)]
    status: PlatformStatus,
    /// Kredensial aplikasi sudah diisi di server atau belum. Dipakai UI
    /// untuk membedakan "belum dihubungkan" dari "belum bisa dihubungkan".
    is_configured: bool,
}

async fn list_platforms(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<PlatformDto>>> {
    user.require(&[Role::Owner])?;

    let rows = repo::list_platforms(&state.pool).await?;
    let configured = state.config.tiktok.is_configured();

    Ok(Json(
        rows.into_iter()
            .map(|status| PlatformDto {
                is_configured: status.platform_name == NAMA_PLATFORM && configured,
                status,
            })
            .collect(),
    ))
}

async fn connect(State(state): State<AppState>, user: CurrentUser) -> AppResult<Redirect> {
    user.require(&[Role::Owner])?;

    if !state.config.tiktok.is_configured() {
        return Err(AppError::bad_request(
            "Kredensial TikTok Shop belum diisi di server (TIKTOK_APP_KEY, TIKTOK_APP_SECRET, TIKTOK_HOST).",
        ));
    }

    // `state` mengikat permintaan otorisasi ini dengan callback-nya. TikTok
    // mengembalikannya apa adanya, jadi callback yang datang tanpa state
    // yang kita kenali bisa ditolak.
    let state_token = Uuid::new_v4().to_string();
    let url = tiktok::auth::url_otorisasi(&state.config.tiktok, &state_token)?;

    Ok(Redirect::temporary(&url))
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    #[serde(default)]
    app_key: Option<String>,
}

/// Dipanggil browser Owner setelah menyetujui izin di Partner Center.
///
/// Tidak memakai extractor `CurrentUser`: yang mengarahkan ke sini adalah
/// TikTok, dan cookie sesi aplikasi tidak ikut terbawa. Yang membuktikan
/// permintaan ini sah adalah `auth_code` -- kode sekali pakai yang hanya
/// bisa ditukar menjadi token oleh pemegang app secret.
async fn callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Redirect> {
    let _ = q.app_key;

    let code = q
        .code
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("TikTok tidak mengirim auth code."))?;

    tiktok::auth::tukar_kode_dengan_token(
        &state.pool,
        &state.config.tiktok,
        &state.config.token_encryption_key,
        &code,
    )
    .await?;

    Ok(Redirect::to("/pengaturan/platform?terhubung=1"))
}

async fn disconnect(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<()>> {
    user.require(&[Role::Owner])?;
    repo::disconnect_platform(&state.pool, NAMA_PLATFORM).await?;
    Ok(Json(()))
}

// ---------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct WebhookBody {
    #[serde(default)]
    data: Option<WebhookData>,
}

#[derive(Debug, Deserialize)]
struct WebhookData {
    #[serde(default)]
    order_id: Option<String>,
}

/// Menerima dorongan order dari TikTok Shop.
///
/// Tiga hal yang menentukan bentuk handler ini:
///
/// 1. **Tanda tangan diperiksa atas body MENTAH**, sebelum di-parse. Body
///    yang sudah melewati serde bukan lagi byte yang ditandatangani -- kunci
///    yang berbeda urutannya saja sudah menghasilkan tanda tangan berbeda.
/// 2. **Jawabannya harus cepat.** Kalau kita lambat, TikTok menganggap
///    pengiriman gagal. Karena tidak ada polling sebagai jaring pengaman,
///    order yang dianggap gagal terkirim bisa hilang selamanya.
/// 3. **Isi webhook tidak dipercaya sebagai sumber data.** Yang dibaca
///    hanyalah id ordernya; detail lengkapnya diambil sendiri dari API
///    TikTok dengan token kita.
async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<&'static str> {
    let cfg = &state.config.tiktok;

    if !cfg.is_configured() {
        return Err(AppError::bad_request(
            "Kredensial TikTok Shop belum diisi di server.",
        ));
    }

    let raw = std::str::from_utf8(&body)
        .map_err(|_| AppError::bad_request("Body webhook bukan UTF-8."))?;

    let signature = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if !tiktok::signature::webhook_sah(&cfg.app_key, &cfg.app_secret, raw, signature) {
        // Sengaja tidak menjelaskan bagian mana yang salah: pengirim yang
        // sah tidak pernah butuh penjelasan itu, dan yang tidak sah tidak
        // perlu dibantu menebak.
        tracing::warn!("webhook TikTok ditolak: tanda tangan tidak cocok");
        return Err(AppError::unauthorized("Tanda tangan webhook tidak sah."));
    }

    let parsed: WebhookBody = serde_json::from_str(raw)
        .map_err(|_| AppError::bad_request("Body webhook bukan JSON yang dikenali."))?;

    let Some(order_id) = parsed.data.and_then(|d| d.order_id) else {
        // Webhook yang tidak membawa order (mis. event lain) tetap dijawab
        // 200 supaya TikTok tidak mengirimnya ulang terus-menerus.
        return Ok("ok");
    };

    let kredensial =
        tiktok::auth::token_yang_berlaku(&state.pool, cfg, &state.config.token_encryption_key)
            .await?;

    let Some(detail) = tiktok::client::detail_order(cfg, &kredensial, &order_id).await? else {
        tracing::warn!(order_id, "detail order tidak ditemukan di TikTok");
        return Ok("ok");
    };

    simpan_order(&state, detail).await?;

    Ok("ok")
}

/// Menormalkan lalu menyimpan satu order. Dipakai webhook, dan dipisah
/// supaya jalur uji bisa memakainya tanpa melalui HTTP.
async fn simpan_order(state: &AppState, detail: serde_json::Value) -> AppResult<Uuid> {
    let order: tiktok::TiktokOrder = serde_json::from_value(detail.clone())
        .map_err(|_| AppError::bad_request("Bentuk order dari TikTok tidak dikenali."))?;

    let normal = tiktok::normalisasi(&order, detail);

    let platform_id = repo::find_platform_id(&state.pool, NAMA_PLATFORM)
        .await?
        .ok_or_else(|| AppError::not_found("Platform tiktok belum terdaftar di database."))?;

    let sla = klasifikasi_sla(normal.shipping_carrier.as_deref());
    let hasil = repo::upsert_order(
        &state.pool,
        platform_id,
        &normal,
        sla.as_str(),
        tenggat_sla(Utc::now(), sla),
    )
    .await?;

    tracing::info!(
        order_id = %normal.external_order_id,
        dibuat = hasil.dibuat,
        "order marketplace tersimpan"
    );

    Ok(hasil.id)
}

// ---------------------------------------------------------------------
// Pesanan
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OrderListQuery {
    status: Option<String>,
}

async fn list_orders(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<OrderListQuery>,
) -> AppResult<Json<Vec<Order>>> {
    let rows = repo::list_orders(&state.pool, q.status.as_deref(), 100).await?;
    Ok(Json(rows))
}

async fn get_order(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Order>> {
    let order = repo::find_order(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Pesanan tidak ditemukan."))?;

    Ok(Json(order))
}

// ---------------------------------------------------------------------
// Pemetaan listing ke produk
// ---------------------------------------------------------------------

async fn list_mappings(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(product_id): Path<Uuid>,
) -> AppResult<Json<Vec<Mapping>>> {
    user.require(&[Role::Owner])?;
    let rows = repo::list_mappings(&state.pool, product_id).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct MappingRequest {
    /// Nama platform, mis. "tiktok".
    platform: String,
    external_item_id: String,
    external_sku: Option<String>,
}

async fn create_mapping(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(product_id): Path<Uuid>,
    Json(body): Json<MappingRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Mapping>)> {
    user.require(&[Role::Owner])?;

    let external_item_id = body.external_item_id.trim();
    if external_item_id.is_empty() {
        return Err(AppError::bad_request("ID listing marketplace wajib diisi."));
    }

    let platform_id = repo::find_platform_id(&state.pool, body.platform.trim())
        .await?
        .ok_or_else(|| AppError::not_found("Platform tidak dikenal."))?;

    repo::upsert_mapping(
        &state.pool,
        platform_id,
        product_id,
        external_item_id,
        body.external_sku
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .await?;

    let mappings = repo::list_mappings(&state.pool, product_id).await?;
    let dibuat = mappings
        .into_iter()
        .find(|m| m.external_item_id == external_item_id)
        .ok_or_else(|| AppError::not_found("Pemetaan tidak ditemukan setelah disimpan."))?;

    Ok((axum::http::StatusCode::CREATED, Json(dibuat)))
}

async fn delete_mapping(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((product_id, mapping_id)): Path<(Uuid, Uuid)>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    if !repo::delete_mapping(&state.pool, product_id, mapping_id).await? {
        return Err(AppError::not_found("Pemetaan tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
