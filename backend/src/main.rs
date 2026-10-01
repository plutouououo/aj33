mod auth;
mod catalog;
mod config;
mod customers;
mod db;
mod error;
mod import;
mod marketplace;
mod orders;
mod pos;
mod reports;
mod stock;
mod tickets;

use axum::http::{HeaderValue, Method};
use axum::routing::get;
use axum::Router;
use sqlx::PgPool;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Segalanya yang dibutuhkan handler, dibagikan lewat state Axum.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: std::sync::Arc<config::Config>,
    /// Pembatas login dibagikan ke semua permintaan, isinya di memori sehingga hilang saat restart dan itu tak apa karena penyerang tak bisa memaksa restart.
    pub throttle: std::sync::Arc<auth::Throttle>,
}

#[tokio::main]
async fn main() {
    // Memuat `backend/.env` bila ada, paling awal agar RUST_LOG terbaca; galat dibuang karena di produksi setelan dari systemd dan `dotenvy` tak menimpa variabel yang sudah diset.
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aj33_backend=debug,tower_http=debug".into()),
        )
        .init();

    let config = match config::Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            // Sengaja tak memakai tracing::error! karena terjadi sebelum aplikasi hidup dan pesannya harus terlihat walau filter log diam.
            eprintln!("Konfigurasi tidak lengkap: {err}");
            std::process::exit(1);
        }
    };

    let pool = match db::connect(&config.database_url).await {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("Gagal terhubung ke database: {err}");
            std::process::exit(1);
        }
    };

    // Migrasi sebelum port dibuka agar versi baru tak menerima trafik di atas skema lama; aman tiap start karena sqlx mencatat di `_sqlx_migrations`.
    if let Err(err) = sqlx::migrate!("../db/migrations").run(&pool).await {
        eprintln!("Migrasi database gagal: {err}");
        std::process::exit(1);
    }

    let port = config.port;
    let cors = build_cors(&config.cors_origins);

    let state = AppState {
        pool,
        config: std::sync::Arc::new(config),
        throttle: std::sync::Arc::new(auth::Throttle::baru()),
    };

    // Worker impor berjalan di samping request (lihat `import::worker`); `state` masih dipakai `.with_state(state)`, jadi disalin (`Clone`), bukan dipindah.
    tokio::spawn(import::jalankan_worker(state.clone()));

    let app = Router::new()
        .route("/api/health", get(health))
        .nest("/api", auth::router())
        .nest("/api", catalog::router())
        .nest("/api", customers::router())
        .nest("/api", import::router())
        .nest("/api", orders::router())
        .nest("/api", pos::router())
        .nest("/api", reports::router())
        .nest("/api", tickets::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("Gagal membuka port {port}: {err}");
            std::process::exit(1);
        }
    };

    tracing::info!("backend siap di http://{addr}");
    axum::serve(listener, app).await.expect("server berhenti");
}

fn build_cors(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::warn!("CORS origin '{o}' diabaikan karena tidak valid");
                None
            }
        })
        .collect();

    CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        // `allow_credentials(true)` tak boleh dipasangkan wildcard (spesifikasi CORS, tower-http panic saat start), jadi header disebut satu per satu.
        .allow_credentials(true)
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::HeaderName::from_static("idempotency-key"),
        ])
}

/// Dipakai healthcheck Docker dan CD; ikut memeriksa database karena backend yang hidup tapi tak bisa query sama dengan mati.
async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<&'static str, error::AppError> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok("ok")
}
