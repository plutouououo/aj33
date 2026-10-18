//! Endpoint pencatat jejak UI.

use super::repo;
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

pub fn router() -> Router<AppState> {
    Router::new().route("/jejak", post(catat))
}

/// Browser mengirim sekali per halaman (saat ditinggalkan), jadi lebih dari ini pasti bukan dari skrip AppShell.
const JEJAK_MAKS: usize = 100;

/// Mengikuti lebar kolom di migrasi 0026; teks tombol yang lebih panjang dipotong, bukan ditolak, karena jejak tak layak menggagalkan apa pun.
const HALAMAN_MAKS: usize = 200;
const AREA_MAKS: usize = 50;
const TARGET_MAKS: usize = 200;

#[derive(Debug, Deserialize)]
struct Jejak {
    halaman: String,
    area: Option<String>,
    target: String,
}

fn potong(s: &str, maks: usize) -> String {
    s.trim().chars().take(maks).collect()
}

/// Terbuka untuk semua peran login karena yang diukur justru pemakaian tiap peran.
async fn catat(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(daftar): Json<Vec<Jejak>>,
) -> AppResult<StatusCode> {
    if daftar.len() > JEJAK_MAKS {
        return Err(AppError::bad_request(format!(
            "Paling banyak {JEJAK_MAKS} jejak sekali kirim."
        )));
    }

    let mut halaman = Vec::with_capacity(daftar.len());
    let mut area = Vec::with_capacity(daftar.len());
    let mut target = Vec::with_capacity(daftar.len());
    for j in &daftar {
        let t = potong(&j.target, TARGET_MAKS);
        if t.is_empty() {
            continue;
        }
        halaman.push(potong(&j.halaman, HALAMAN_MAKS));
        area.push(potong(j.area.as_deref().unwrap_or(""), AREA_MAKS));
        target.push(t);
    }

    if !target.is_empty() {
        repo::simpan(&state.pool, user.id, &halaman, &area, &target).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}
