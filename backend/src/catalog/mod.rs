//! Produk, varian, batch, kategori, dan riwayat perubahan stok.

mod repo;
mod routes;
pub(crate) mod service;
mod sku;

pub use routes::router;
