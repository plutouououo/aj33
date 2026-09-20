//! Endpoint auth + extractor yang dipakai seluruh modul lain.
//!
//! Backend hanya mengenal `Authorization: Bearer <token>`. Urusan cookie
//! sepenuhnya milik frontend Astro: ia membaca cookie httpOnly saat merender
//! di server, lalu meneruskannya ke sini sebagai Bearer. Dengan begitu hanya
//! ada satu cara autentikasi yang perlu dijaga di backend.

use super::service::{self, PublicUser};
use super::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
}

impl CurrentUser {
    /// Menegakkan batas peran. Dipanggil di awal handler yang terbatas:
    ///
    /// ```ignore
    /// user.require(&[Role::Owner])?;
    /// ```
    pub fn require(&self, allowed: &[Role]) -> AppResult<()> {
        if allowed.contains(&self.role) {
            Ok(())
        } else {
            Err(AppError::forbidden(
                "Kamu tidak punya akses ke halaman/fitur ini.",
            ))
        }
    }
}

/// Handler yang menuliskan `CurrentUser` di parameternya otomatis terlindungi:
/// request tanpa token valid tidak akan pernah sampai ke badan fungsinya.
/// Ini membuat "lupa memasang middleware auth" menjadi mustahil, berbeda
/// dengan `requireAuth` yang harus diingat pada tiap route di proyek lama.
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::unauthorized("Harus login dulu."))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::unauthorized("Harus login dulu."))?;

        service::baca_token(&state.config.jwt_secret, token)
    }
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email_or_username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
    user: PublicUser,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    let (token, user) = service::login(
        &state.pool,
        &state.config.jwt_secret,
        &state.throttle,
        body.email_or_username.trim(),
        &body.password,
    )
    .await?;

    Ok(Json(LoginResponse { token, user }))
}

/// Token JWT tidak bisa dicabut dari sisi server tanpa menyimpan daftar token
/// yang dibatalkan, dan proyek lama pun tidak melakukannya. Yang benar-benar
/// mengakhiri sesi adalah frontend yang menghapus cookie-nya. Endpoint ini
/// tetap ada supaya frontend punya satu tempat memanggil saat logout dan
/// kontrak API tidak berubah.
async fn logout(_user: CurrentUser) -> AppResult<axum::http::StatusCode> {
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<PublicUser>> {
    let me = service::get_me(&state.pool, user.id).await?;
    Ok(Json(me))
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// Ganti password sendiri. Tidak ada peran yang dikecualikan: akun dengan
/// password sementara justru TIDAK BISA mengerjakan apa pun sebelum lewat
/// sini, jadi membatasinya per peran hanya akan mengunci orang keluar.
async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ChangePasswordRequest>,
) -> AppResult<Json<PublicUser>> {
    let updated = service::change_password(
        &state.pool,
        user.id,
        &body.current_password,
        &body.new_password,
    )
    .await?;

    Ok(Json(updated))
}
