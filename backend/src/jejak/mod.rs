//! Jejak interaksi UI (klik tombol dan menu) yang dikirim browser lewat server Astro, disimpan apa adanya untuk dianalisis dengan SQL.

mod repo;
mod routes;

pub use routes::router;
