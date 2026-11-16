//! Menjalankan rencana pratinjau lewat `catalog::service` dan `stock`, jalur yang sama dengan form manual, per baris agar satu baris gagal tak menahan yang lain.

use super::{judul, Aksi, Baris, HasilBaris, Kolom, KolomBatch, Perubahan};
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
            } => ubah_batch(state, user_id, *produk_id, *batch_id, kolom, &b.perubahan).await,
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
        wholesale_threshold_qty: None,
        wholesale_threshold_qty_shopee: None,
        wholesale_threshold_qty_tiktok: None,
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
            Kolom::Batas(v) => n.wholesale_threshold_qty = v,
            Kolom::BatasShopee(v) => n.wholesale_threshold_qty_shopee = v,
            Kolom::BatasTiktok(v) => n.wholesale_threshold_qty_tiktok = v,
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
            Kolom::Batas(v) => patch.wholesale_threshold_qty = Some(v),
            Kolom::BatasShopee(v) => patch.wholesale_threshold_qty_shopee = Some(v),
            Kolom::BatasTiktok(v) => patch.wholesale_threshold_qty_tiktok = Some(v),
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
    perubahan: &[Perubahan],
) -> AppResult<()> {
    let mut tx = state.pool.begin().await?;
    let batch = repo::kunci_batch(&mut tx, produk_id, batch_id)
        .await?
        .ok_or_else(|| AppError::not_found("Batch tidak ditemukan."))?;

    let mut patch = BatchPatch::default();
    let mut target = None;
    let mut alasan = None;
    for k in kolom {
        let k = k.clone();
        match k {
            KolomBatch::Sisa(v) => target = Some(v),
            KolomBatch::Alasan(v) => alasan = Some(v),
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
        periksa_sisa_pratinjau(perubahan, batch.remaining_qty)?;
        let selisih = target - batch.remaining_qty;
        if selisih != 0 {
            let baris = StockLine {
                product_id: produk_id,
                qty: selisih.abs(),
                batch_id: Some(batch_id),
            };
            if selisih > 0 {
                stock::tambah(
                    &mut tx,
                    &[baris],
                    StockReason::ManualAdjustment(None),
                    produk_id,
                    Some(user_id),
                )
                .await?;
            } else {
                // Pratinjau lama tanpa kolom alasan ditolak di sini, memakai aturan yang sama dengan koreksi manual.
                let alasan_kurang = stock::alasan_koreksi(selisih, alasan.map(|a| a.as_str()))
                    .map_err(AppError::bad_request)?;
                stock::kurangi(
                    &mut tx,
                    &[baris],
                    StockReason::ManualAdjustment(alasan_kurang),
                    produk_id,
                    Some(user_id),
                )
                .await?;
            }
        }
    }
    tx.commit().await?;

    let ada_metadata = patch.expiry_date.is_some()
        || patch.storage_location.is_some()
        || patch.purchase_price.is_some();
    if ada_metadata {
        repo::update_batch(&state.pool, produk_id, batch_id, &patch).await?;
    }

    Ok(())
}

/// Menolak selisih bila sisa batch sudah bergeser sejak pratinjau, supaya selisih lama tak membatalkan penjualan di antaranya.
fn periksa_sisa_pratinjau(perubahan: &[Perubahan], sisa_kini: i32) -> AppResult<()> {
    let pratinjau = perubahan
        .iter()
        .find(|p| p.kolom == judul::SISA)
        .and_then(|p| p.lama.trim().parse::<i32>().ok());

    match pratinjau {
        Some(lama) if lama == sisa_kini => Ok(()),
        Some(lama) => Err(AppError::bad_request(format!(
            "Sisa batch sudah berubah sejak pratinjau (dulu {lama}, sekarang {sisa_kini}). Unduh ulang berkas edit massal stok, lalu unggah ulang."
        ))),
        None => Err(AppError::bad_request(
            "Pratinjau tidak memuat sisa stok batch ini. Unduh ulang berkas edit massal stok, lalu unggah ulang.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sisa_lama(lama: &str) -> Vec<Perubahan> {
        vec![Perubahan {
            kolom: judul::SISA.to_string(),
            lama: lama.to_string(),
            baru: "10".to_string(),
        }]
    }

    #[test]
    fn sisa_yang_masih_sama_dengan_pratinjau_diterapkan() {
        assert!(periksa_sisa_pratinjau(&sisa_lama("8"), 8).is_ok());
    }

    #[test]
    fn sisa_yang_sudah_bergeser_ditolak() {
        // Penjualan di antara pratinjau dan terapkan mengubah sisa dari 8 menjadi 6.
        assert!(periksa_sisa_pratinjau(&sisa_lama("8"), 6).is_err());
    }

    #[test]
    fn pratinjau_tanpa_sisa_atau_tak_terbaca_ditolak() {
        assert!(periksa_sisa_pratinjau(&[], 8).is_err());
        assert!(periksa_sisa_pratinjau(&sisa_lama("delapan"), 8).is_err());
    }
}
