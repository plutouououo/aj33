//! Worker latar pertama: polling `FOR UPDATE SKIP LOCKED` (Postgres cukup, volume rendah); tiap baris commit di transaksinya sendiri sehingga restart tak memproses ulang; aman untuk satu instance.

use super::repo;
use crate::AppState;
use std::time::Duration;

/// Menerima `AppState` lengkap karena `catalog::service::create_product`/`update_product` mengambil `&AppState`; `main.rs` mengirim salinannya.
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
