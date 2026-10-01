//! Ringkasan dasbor dan laporan: agregasi Postgres atas transaksi, produk, dan beban; "hari ini" memakai `AT TIME ZONE 'Asia/Jakarta'` di tiap SQL (ubah bersamaan bila zona berubah).

mod repo;
mod routes;

pub use routes::router;
