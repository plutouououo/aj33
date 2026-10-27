//! Menulis berkas ekspor edit massal; kolom pertama adalah kunci baca-saja yang dipakai `rencana` mencocokkan baris.

use super::judul as j;
use crate::catalog::repo::{Product, ProductBatch};
use crate::error::{AppError, AppResult};
use chrono::{Datelike, NaiveDate};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_xlsxwriter::{Color, ExcelDateTime, Format, Workbook, Worksheet, XlsxError};
use std::collections::HashMap;
use uuid::Uuid;

/// Satu sel yang akan ditulis; `Kosong` melewati sel agar tampil benar-benar kosong.
enum Sel {
    Teks(String),
    Angka(f64),
    Tanggal(NaiveDate),
    Kosong,
}

fn teks(nilai: &Option<String>) -> Sel {
    nilai.clone().map_or(Sel::Kosong, Sel::Teks)
}

fn angka(nilai: Option<Decimal>) -> Sel {
    nilai
        .and_then(|v| v.to_f64())
        .map_or(Sel::Kosong, Sel::Angka)
}

fn galat(err: XlsxError) -> AppError {
    AppError::internal(format!("Gagal menulis berkas xlsx: {err}"))
}

/// `baca_saja`: kolom pertama sebanyak ini diberi latar abu-abu di header sebagai penanda bahwa isinya acuan, bukan untuk disunting.
fn tulis(
    nama_sheet: &str,
    judul: &[&str],
    baca_saja: usize,
    baris: Vec<Vec<Sel>>,
) -> AppResult<Vec<u8>> {
    let mut buku = Workbook::new();
    let lembar: &mut Worksheet = buku.add_worksheet();
    lembar.set_name(nama_sheet).map_err(galat)?;

    let header = Format::new().set_bold();
    let header_acuan = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xD9D9D9));
    let tanggal = Format::new().set_num_format("yyyy-mm-dd");

    for (c, teks) in judul.iter().enumerate() {
        let format = if c < baca_saja {
            &header_acuan
        } else {
            &header
        };
        lembar
            .write_string_with_format(0, c as u16, *teks, format)
            .map_err(galat)?;
    }

    for (i, isi) in baris.into_iter().enumerate() {
        let r = (i + 1) as u32;
        for (c, sel) in isi.into_iter().enumerate() {
            let c = c as u16;
            match sel {
                Sel::Teks(t) => lembar.write_string(r, c, t).map(|_| ()),
                Sel::Angka(n) => lembar.write_number(r, c, n).map(|_| ()),
                Sel::Tanggal(t) => {
                    ExcelDateTime::from_ymd(t.year() as u16, t.month() as u8, t.day() as u8)
                        .and_then(|d| {
                            lembar
                                .write_datetime_with_format(r, c, &d, &tanggal)
                                .map(|_| ())
                        })
                }
                Sel::Kosong => Ok(()),
            }
            .map_err(galat)?;
        }
    }

    lembar.set_freeze_panes(1, 0).map_err(galat)?;
    lembar.autofit();
    buku.save_to_buffer().map_err(galat)
}

pub fn utama(produk: &[Product]) -> AppResult<Vec<u8>> {
    let judul = [
        j::ID,
        j::SKU,
        j::NAMA,
        j::NAMA_SEO,
        j::KATEGORI,
        j::JENIS,
        j::GRADE,
        j::MEREK,
        j::UKURAN,
        j::HARGA,
        j::GROSIR,
        j::SHOPEE,
        j::GROSIR_SHOPEE,
        j::TIKTOK,
        j::GROSIR_TIKTOK,
        j::BATAS,
        j::BATAS_SHOPEE,
        j::BATAS_TIKTOK,
        j::BATAS_STOK,
        j::STATUS,
    ];
    let baris = produk
        .iter()
        .map(|p| {
            vec![
                Sel::Teks(p.id.to_string()),
                teks(&p.sku),
                Sel::Teks(p.name.clone()),
                teks(&p.seo_name),
                teks(&p.category_name),
                teks(&p.product_type),
                teks(&p.variant_grade),
                teks(&p.brand_name),
                angka(p.variant_size),
                angka(Some(p.price)),
                angka(p.price_wholesale),
                angka(p.price_shopee),
                angka(p.price_shopee_wholesale),
                angka(p.price_tiktok),
                angka(p.price_tiktok_wholesale),
                angka(p.wholesale_threshold_kg),
                angka(p.wholesale_threshold_kg_shopee),
                angka(p.wholesale_threshold_kg_tiktok),
                Sel::Angka(f64::from(p.low_stock_threshold)),
                Sel::Teks(if p.is_active { "aktif" } else { "nonaktif" }.to_string()),
            ]
        })
        .collect();

    tulis("Produk", &judul, 1, baris)
}

/// Satu baris per batch yang masih bersisa; `id` produk induk tidak ikut diekspor sehingga induk di luar saringan tampil kosong.
pub fn stok(produk: &[Product], batch: &[ProductBatch]) -> AppResult<Vec<u8>> {
    let judul = [
        j::ID_BATCH,
        j::SKU,
        j::NAMA,
        j::SKU_INDUK,
        j::GRADE,
        j::UKURAN,
        j::NO_BATCH,
        j::ISI_KIRIMAN,
        j::SISA,
        j::KEDALUWARSA,
        j::RAK,
        j::HARGA_BELI,
    ];
    let per_produk: HashMap<Uuid, &Product> = produk.iter().map(|p| (p.id, p)).collect();

    let baris = batch
        .iter()
        .filter_map(|b| {
            let p = per_produk.get(&b.product_id)?;
            let induk = p
                .parent_id
                .and_then(|id| per_produk.get(&id))
                .and_then(|i| i.sku.clone());
            Some(vec![
                Sel::Teks(b.id.to_string()),
                teks(&p.sku),
                Sel::Teks(p.name.clone()),
                teks(&induk),
                teks(&p.variant_grade),
                angka(p.variant_size),
                teks(&b.batch_number),
                Sel::Angka(f64::from(b.quantity)),
                Sel::Angka(f64::from(b.remaining_qty)),
                b.expiry_date.map_or(Sel::Kosong, Sel::Tanggal),
                teks(&b.storage_location),
                angka(b.purchase_price),
            ])
        })
        .collect();

    // Delapan kolom pertama (id batch sampai isi kiriman) hanya acuan; yang bisa disunting mulai dari sisa stok.
    tulis("Stok", &judul, 8, baris)
}
