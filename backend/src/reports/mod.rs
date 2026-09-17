//! Angka ringkasan untuk dasbor dan laporan penjualan.
//!
//! Modul ini tidak punya tabel sendiri: seluruh isinya adalah agregasi di
//! atas `transactions`, `transaction_items`, `products`, dan `expenses`.
//! Agregasinya dikerjakan Postgres, bukan ditarik mentah lalu dijumlahkan di
//! Rust -- satu tahun transaksi tidak perlu melewati jaringan hanya untuk
//! menghasilkan empat angka.
//!
//! ZONA WAKTU. "Hari ini" bagi pemilik toko adalah hari di Yogyakarta, bukan
//! hari UTC tempat server kebetulan berjalan: laporan harian yang berganti
//! pukul 07.00 pagi memotong penjualan pagi ke hari sebelumnya. Karena itu
//! setiap batas waktu di `repo.rs` ditulis `AT TIME ZONE 'Asia/Jakarta'` --
//! nama zona IANA, bukan offset +07:00, supaya Postgres yang memegang
//! aturannya. Nilainya tertanam di tiap SQL karena `sqlx::query!` hanya
//! menerima SQL literal; kalau zona toko berubah, semuanya harus berubah
//! bersamaan.

mod repo;
mod routes;

pub use routes::router;
