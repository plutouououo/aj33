//! Beban operasional toko (tabel `expenses`): dicatat owner, dibaca laporan laba rugi dan laporan harian Sheets.

mod repo;
mod routes;

pub use routes::router;
