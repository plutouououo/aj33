//! Langganan Web Push per perangkat; yang mengirim notifikasi server Astro (pustaka `web-push`), backend hanya menyimpan alamat langganannya.

mod repo;
mod routes;

pub use routes::router;
