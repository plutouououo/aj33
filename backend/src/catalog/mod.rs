//! Produk, varian, batch, kategori, dan riwayat perubahan stok.

mod massal;
mod repo;
mod routes;
pub(crate) mod service;
mod sku;

/// Rute katalog digabung dengan rute edit massal agar `main` tetap memasang satu router per modul.
pub fn router() -> axum::Router<crate::AppState> {
    routes::router().merge(massal::router())
}
