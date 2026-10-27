//! Menjalankan rencana pratinjau lewat `catalog::service` dan `stock`, jalur yang sama dengan form manual, per baris agar satu baris gagal tak menahan yang lain.

use super::{Aksi, Baris, HasilBaris, Kolom, KolomBatch};
use crate::catalog::repo::{self, BatchPatch, ProductPatch};
use crate::catalog::service::{self, CreateProductInput};
use crate::error::{AppError, AppResult};
use crate::stock::{self, StockLine, StockReason};
use crate::AppState;
use rust_decimal::Decimal;
use uuid::Uuid;

pub async fn jalankan(state: &AppState, user_id: Uuid, baris: &[Baris]) -> Vec<HasilBaris> {
    let mut hasil = Vec::new();
    for b in baris {
        let Some(aksi) = &b.aksi else { continue };
        let jalan = match aksi {
            Aksi::Buat(kolom) => buat(state, user_id, kolom).await,
            Aksi::Ubah { id, kolom } => ubah(state, *id, kolom).await,
            Aksi::UbahBatch {
                produk_id,
                batch_id,
                kolom,
            } => ubah_batch(state, user_id, *produk_id, *batch_id, kolom).await,
        };
        hasil.push(HasilBaris {
            no: b.no,
            ok: jalan.is_ok(),
            pesan: jalan.err().map(|err| err.to_string()),
        });
    }
    hasil
}

async fn buat(state: &AppState, user_id: Uuid, kolom: &[Kolom]) -> AppResult<()> {
    let mut n = CreateProductInput {
        name: String::new(),
        seo_name: None,
        brand_name: None,
        product_type: None,
        variant_grade: None,
        variant_size: None,
        parent_id: None,
        category_id: None,
        price: Decimal::ZERO,
        price_wholesale: None,
        price_shopee: None,
        price_tiktok: None,
        price_shopee_wholesale: None,
        price_tiktok_wholesale: None,
        wholesale_threshold_kg: None,
        wholesale_threshold_kg_shopee: None,
        wholesale_threshold_kg_tiktok: None,
        cost_price: None,
        is_active: true,
        low_stock_threshold: None,
        image_url: None,
        stock_qty: 0,
        purchase_price: None,
        batch_number: None,
        expiry_date: None,
        storage_location: None,
    };

    for k in kolom {
        let k = k.clone();
        match k {
            Kolom::Nama(v) => n.name = v,
            Kolom::NamaSeo(v) => n.seo_name = v,
            // SKU produk baru selalu dirakit sistem dari jenis, grade, merek, dan ukuran.
            Kolom::Sku(_) => {}
            Kolom::Kategori(v) => n.category_id = v,
            Kolom::Jenis(v) => n.product_type = v,
            Kolom::Grade(v) => n.variant_grade = v,
            Kolom::Merek(v) => n.brand_name = v,
            Kolom::Ukuran(v) => n.variant_size = v,
            Kolom::Harga(v) => n.price = v,
            Kolom::Grosir(v) => n.price_wholesale = v,
            Kolom::Shopee(v) => n.price_shopee = v,
            Kolom::GrosirShopee(v) => n.price_shopee_wholesale = v,
            Kolom::Tiktok(v) => n.price_tiktok = v,
            Kolom::GrosirTiktok(v) => n.price_tiktok_wholesale = v,
            Kolom::Batas(v) => n.wholesale_threshold_kg = v,
            Kolom::BatasShopee(v) => n.wholesale_threshold_kg_shopee = v,
            Kolom::BatasTiktok(v) => n.wholesale_threshold_kg_tiktok = v,
            Kolom::BatasStok(v) => n.low_stock_threshold = Some(v),
            Kolom::Aktif(v) => n.is_active = v,
        }
    }

    service::create_product(state, user_id, n).await.map(|_| ())
}

async fn ubah(state: &AppState, id: Uuid, kolom: &[Kolom]) -> AppResult<()> {
    let sekarang = service::ambil_produk(state, id).await?;
    let mut patch = ProductPatch::default();

    for k in kolom {
        let k = k.clone();
        match k {
            Kolom::Nama(v) => patch.name = Some(v),
            Kolom::NamaSeo(v) => patch.seo_name = Some(v),
            Kolom::Sku(v) => {
                patch.sku = Some(Some(service::koreksi_sku(state, &sekarang, &v).await?));
            }
            Kolom::Kategori(v) => patch.category_id = Some(v),
            Kolom::Jenis(v) => patch.product_type = Some(v),
            Kolom::Grade(v) => patch.variant_grade = Some(v),
            Kolom::Merek(v) => patch.brand_name = Some(v),
            Kolom::Ukuran(v) => patch.variant_size = Some(v),
            Kolom::Harga(v) => patch.price = Some(v),
            Kolom::Grosir(v) => patch.price_wholesale = Some(v),
            Kolom::Shopee(v) => patch.price_shopee = Some(v),
            Kolom::GrosirShopee(v) => patch.price_shopee_wholesale = Some(v),
            Kolom::Tiktok(v) => patch.price_tiktok = Some(v),
            Kolom::GrosirTiktok(v) => patch.price_tiktok_wholesale = Some(v),
            Kolom::Batas(v) => patch.wholesale_threshold_kg = Some(v),
            Kolom::BatasShopee(v) => patch.wholesale_threshold_kg_shopee = Some(v),
            Kolom::BatasTiktok(v) => patch.wholesale_threshold_kg_tiktok = Some(v),
            Kolom::BatasStok(v) => patch.low_stock_threshold = Some(v),
            Kolom::Aktif(v) => patch.is_active = Some(v),
        }
    }

    service::update_product(state, id, patch).await.map(|_| ())
}

/// Stok diubah dulu (divalidasi `stock.rs` dan tercatat di ledger sebagai koreksi manual), lalu metadata batch.
async fn ubah_batch(
    state: &AppState,
    user_id: Uuid,
    produk_id: Uuid,
    batch_id: Uuid,
    kolom: &[KolomBatch],
) -> AppResult<()> {
    let batch = {
        let mut tx = state.pool.begin().await?;
        repo::find_batch(&mut tx, produk_id, batch_id)
            .await?
            .ok_or_else(|| AppError::not_found("Batch tidak ditemukan."))?
    };

    let mut patch = BatchPatch::default();
    let mut target = None;
    for k in kolom {
        let k = k.clone();
        match k {
            KolomBatch::Sisa(v) => target = Some(v),
            KolomBatch::Kedaluwarsa(v) => patch.expiry_date = Some(v),
            KolomBatch::Rak(v) => patch.storage_location = Some(v),
            KolomBatch::HargaBeli(v) => patch.purchase_price = Some(v),
        }
    }

    if let Some(target) = target {
        if target < 0 || target > batch.quantity {
            return Err(AppError::bad_request(format!(
                "Sisa stok harus antara 0 dan isi kiriman ({}).",
                batch.quantity
            )));
        }
        let selisih = target - batch.remaining_qty;
        if selisih != 0 {
            let baris = StockLine {
                product_id: produk_id,
                qty: selisih.abs(),
                batch_id: Some(batch_id),
            };
            let mut tx = state.pool.begin().await?;
            if selisih > 0 {
                stock::tambah(
                    &mut tx,
                    &[baris],
                    StockReason::ManualAdjustment,
                    produk_id,
                    Some(user_id),
                )
                .await?;
            } else {
                stock::kurangi(
                    &mut tx,
                    &[baris],
                    StockReason::ManualAdjustment,
                    produk_id,
                    Some(user_id),
                )
                .await?;
            }
            tx.commit().await?;
        }
    }

    let ada_metadata = patch.expiry_date.is_some()
        || patch.storage_location.is_some()
        || patch.purchase_price.is_some();
    if ada_metadata {
        repo::update_batch(&state.pool, produk_id, batch_id, &patch).await?;
    }

    Ok(())
}
