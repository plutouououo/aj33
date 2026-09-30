//! Worker latar belakang PERTAMA di backend ini -- polling sederhana lewat
//! `FOR UPDATE SKIP LOCKED`, bukan pustaka antrean baru. Volume impor toko
//! tunggal ini rendah, dan Postgres sudah punya seluruh primitif yang
//! dibutuhkan.
//!
//! Sengaja TIDAK menahan kunci baris selama seluruh batch diproses -- tiap
//! baris commit lewat transaksinya sendiri (di dalam `catalog::service`) dan
//! hasilnya (`catat_hasil_baris`) langsung permanen begitu satu baris
//! selesai, bukan menunggu di akhir. Kalau backend mati di tengah jalan,
//! baris yang sudah `ok`/`failed` tidak diproses ulang -- `next_pending_row`
//! cuma mengambil sisa `commit_state='pending'`. Aman untuk satu instance
//! backend (sesuai deploy saat ini); kalau backend ini suatu saat dijalankan
//! lebih dari satu instance sekaligus, dua worker bisa saja sama-sama
//! memoles antrean tanpa membaginya (masing-masing baris tetap aman lewat
//! `FOR UPDATE SKIP LOCKED` di `next_pending_row`, cuma boros, bukan
//! salah).

use super::repo;
use crate::AppState;
use std::time::Duration;

/// Menerima `AppState` lengkap (bukan cuma `PgPool`) karena
/// `catalog::service::create_product`/`update_product` yang dipanggil
/// `service::commit_row` mengambil `&AppState` -- meski yang benar-benar
/// disentuh cuma `state.pool`. `AppState` sudah `Clone`, jadi `main.rs`
/// cukup mengirim salinannya sebelum yang asli dipakai `axum::serve`.
pub async fn jalankan_worker(state: AppState) {
    loop {
        if let Err(err) = satu_putaran(&state).await {
            tracing::error!(error = %err, "putaran worker impor gagal");
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn satu_putaran(state: &AppState) -> crate::error::AppResult<()> {
    let pool = &state.pool;

    let Some(batch_id) = repo::klaim_batch_committing(pool).await? else {
        return Ok(());
    };

    let Some(batch) = repo::baca_batch_ringkas(pool, batch_id).await? else {
        return Ok(());
    };

    repo::tandai_baris_dilewati_selesai(pool, batch_id).await?;

    loop {
        let Some(row) = repo::next_pending_row(pool, batch_id).await? else {
            break;
        };

        let (commit_state, product_id, error) =
            super::service::commit_row(state, &batch, &row).await;
        repo::catat_hasil_baris(pool, row.id, commit_state, product_id, error.as_deref()).await?;
    }

    let hasil = repo::hitung_hasil(pool, batch_id).await?;
    repo::selesaikan_batch(pool, batch_id, hasil.ok, hasil.fail).await?;

    Ok(())
}
