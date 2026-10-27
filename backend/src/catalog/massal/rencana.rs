//! Mengurai berkas xlsx jadi rencana per baris tanpa SQL, dites tanpa database (modul `tests`).

use super::{judul as j, Aksi, Baris, Kolom, KolomBatch, Perubahan, StatusBaris};
use crate::catalog::repo::{Category, Product, ProductBatch};
use crate::catalog::service::bersihkan;
use crate::error::{AppError, AppResult};
use crate::import::parse::{parse_bool, parse_money, parse_ukuran_kg, BATAS_BARIS};
use calamine::{Data, Reader, Xlsx};
use chrono::{Days, NaiveDate};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use uuid::Uuid;

static KOSONG: Data = Data::Empty;

/// Sheet pertama berkas: posisi kolom menurut judulnya, dan baris datanya.
pub struct Lembar {
    kolom: HashMap<String, usize>,
    baris: Vec<Vec<Data>>,
    /// Nomor baris spreadsheet untuk baris data pertama.
    awal: i32,
}

impl Lembar {
    pub fn baca(bytes: &[u8]) -> AppResult<Self> {
        let mut buku: Xlsx<_> = Xlsx::new(Cursor::new(bytes)).map_err(|err| {
            AppError::bad_request(format!("Berkas xlsx tidak bisa dibaca: {err}"))
        })?;
        let nama = buku
            .sheet_names()
            .first()
            .cloned()
            .ok_or_else(|| AppError::bad_request("Berkas xlsx tidak punya sheet."))?;
        let rentang = buku.worksheet_range(&nama).map_err(|err| {
            AppError::bad_request(format!("Sheet pertama tidak bisa dibaca: {err}"))
        })?;

        let mut baris = rentang.rows();
        let header = baris
            .next()
            .ok_or_else(|| AppError::bad_request("Berkas xlsx kosong."))?;
        let kolom = header
            .iter()
            .enumerate()
            .filter_map(|(i, sel)| teks(sel).map(|t| (t.to_lowercase(), i)))
            .collect();
        let baris: Vec<Vec<Data>> = baris.map(<[Data]>::to_vec).collect();
        if baris.len() > BATAS_BARIS {
            return Err(AppError::bad_request(format!(
                "Berkas melebihi batas {BATAS_BARIS} baris."
            )));
        }

        // Rentang calamine mulai dari sel terisi pertama, bukan A1.
        let baris_header = rentang.start().map_or(0, |(b, _)| b as i32) + 1;
        Ok(Self {
            kolom,
            baris,
            awal: baris_header + 1,
        })
    }

    fn ada(&self, judul: &str) -> bool {
        self.kolom.contains_key(judul)
    }

    /// `None` bila kolomnya tak ada di berkas (jangan disentuh); sel kosong tetap `Some`.
    fn sel<'a>(&self, baris: &'a [Data], judul: &str) -> Option<&'a Data> {
        let i = *self.kolom.get(judul)?;
        Some(baris.get(i).unwrap_or(&KOSONG))
    }
}

fn baris_kosong(baris: &[Data]) -> bool {
    baris.iter().all(|sel| teks(sel).is_none())
}

fn teks(sel: &Data) -> Option<String> {
    match sel {
        Data::Empty => None,
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => Some((*f as i64).to_string()),
        lain => bersihkan(Some(lain.to_string())),
    }
}

fn angka(sel: &Data) -> Result<Option<Decimal>, String> {
    match sel {
        Data::Empty => Ok(None),
        Data::Int(i) => Ok(Some(Decimal::from(*i))),
        Data::Float(f) => f
            .to_string()
            .parse::<Decimal>()
            .map(Some)
            .map_err(|_| format!("\"{f}\" bukan angka.")),
        Data::String(s) if s.trim().is_empty() => Ok(None),
        Data::String(s) => parse_money(s)
            .map(Some)
            .ok_or_else(|| format!("\"{}\" bukan angka.", s.trim())),
        lain => Err(format!("\"{lain}\" bukan angka.")),
    }
}

fn uang(sel: &Data) -> Result<Option<Decimal>, String> {
    match angka(sel)? {
        Some(v) if v.is_sign_negative() => Err("tidak boleh negatif.".to_string()),
        Some(v) => Ok(Some(v.round_dp(2))),
        None => Ok(None),
    }
}

/// Angka biasa dibaca sebagai kg; teks seperti "500 gr" lewat `parse_ukuran_kg`.
fn kg(sel: &Data) -> Result<Option<Decimal>, String> {
    if let Data::String(s) = sel {
        if s.trim().is_empty() {
            return Ok(None);
        }
        return parse_ukuran_kg(s)
            .map(|v| Some(v.round_dp(3)))
            .ok_or_else(|| format!("\"{}\" bukan ukuran yang valid.", s.trim()));
    }
    match angka(sel)? {
        Some(v) if v <= Decimal::ZERO => Err("harus lebih dari 0.".to_string()),
        Some(v) => Ok(Some(v.round_dp(3))),
        None => Ok(None),
    }
}

fn bulat(sel: &Data) -> Result<Option<i32>, String> {
    match angka(sel)? {
        None => Ok(None),
        Some(v) if v.fract() != Decimal::ZERO => Err("harus bilangan bulat.".to_string()),
        Some(v) => v
            .to_i32()
            .map(Some)
            .ok_or_else(|| "terlalu besar.".to_string()),
    }
}

fn tanggal(sel: &Data) -> Result<Option<NaiveDate>, String> {
    match sel {
        Data::Empty => Ok(None),
        Data::DateTime(d) => dari_serial(d.as_f64()),
        Data::Float(f) => dari_serial(*f),
        Data::String(s) if s.trim().is_empty() => Ok(None),
        Data::String(s) => ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"]
            .iter()
            .find_map(|format| NaiveDate::parse_from_str(s.trim(), format).ok())
            .map(Some)
            .ok_or_else(|| format!("\"{}\" bukan tanggal (tulis YYYY-MM-DD).", s.trim())),
        lain => Err(format!("\"{lain}\" bukan tanggal.")),
    }
}

/// Nomor seri tanggal Excel dengan 30 Desember 1899 sebagai hari ke-0.
fn dari_serial(serial: f64) -> Result<Option<NaiveDate>, String> {
    NaiveDate::from_ymd_opt(1899, 12, 30)
        .and_then(|awal| awal.checked_add_days(Days::new(serial.max(0.0) as u64)))
        .map(Some)
        .ok_or_else(|| "tanggal di luar rentang.".to_string())
}

fn status(sel: &Data) -> Result<Option<bool>, String> {
    let Some(t) = teks(sel) else { return Ok(None) };
    match t.to_lowercase().as_str() {
        "aktif" => Ok(Some(true)),
        "nonaktif" | "non aktif" | "tidak aktif" => Ok(Some(false)),
        lain => parse_bool(lain)
            .map(Some)
            .ok_or_else(|| format!("\"{t}\" bukan status (isi aktif atau nonaktif).")),
    }
}

fn tampil_teks(nilai: &Option<String>) -> String {
    nilai.clone().unwrap_or_default()
}

fn tampil_angka(nilai: &Option<Decimal>) -> String {
    nilai.map(|v| v.normalize().to_string()).unwrap_or_default()
}

fn tampil_status(aktif: bool) -> String {
    if aktif { "aktif" } else { "nonaktif" }.to_string()
}

/// Atribut utama yang bisa disunting, dikumpulkan dalam satu struct agar baris lama dan hasil bacaan berkas bisa dibandingkan.
#[derive(Clone)]
struct Nilai {
    nama: String,
    seo: Option<String>,
    kategori: Option<Uuid>,
    jenis: Option<String>,
    grade: Option<String>,
    merek: Option<String>,
    ukuran: Option<Decimal>,
    harga: Decimal,
    grosir: Option<Decimal>,
    shopee: Option<Decimal>,
    grosir_shopee: Option<Decimal>,
    tiktok: Option<Decimal>,
    grosir_tiktok: Option<Decimal>,
    batas: Option<Decimal>,
    batas_shopee: Option<Decimal>,
    batas_tiktok: Option<Decimal>,
    batas_stok: i32,
    aktif: bool,
}

impl Nilai {
    /// Titik mulai produk baru; sama dengan bawaan form tambah produk.
    fn awal() -> Self {
        Self {
            nama: String::new(),
            seo: None,
            kategori: None,
            jenis: None,
            grade: None,
            merek: None,
            ukuran: None,
            harga: Decimal::ZERO,
            grosir: None,
            shopee: None,
            grosir_shopee: None,
            tiktok: None,
            grosir_tiktok: None,
            batas: None,
            batas_shopee: None,
            batas_tiktok: None,
            batas_stok: 5,
            aktif: true,
        }
    }

    fn dari(p: &Product) -> Self {
        Self {
            nama: p.name.clone(),
            seo: p.seo_name.clone(),
            kategori: p.category_id,
            jenis: p.product_type.clone(),
            grade: p.variant_grade.clone(),
            merek: p.brand_name.clone(),
            ukuran: p.variant_size,
            harga: p.price,
            grosir: p.price_wholesale,
            shopee: p.price_shopee,
            grosir_shopee: p.price_shopee_wholesale,
            tiktok: p.price_tiktok,
            grosir_tiktok: p.price_tiktok_wholesale,
            batas: p.wholesale_threshold_kg,
            batas_shopee: p.wholesale_threshold_kg_shopee,
            batas_tiktok: p.wholesale_threshold_kg_tiktok,
            batas_stok: p.low_stock_threshold,
            aktif: p.is_active,
        }
    }
}

fn isi<T>(galat: &mut Vec<String>, judul: &str, hasil: Result<T, String>, tujuan: &mut T) {
    match hasil {
        Ok(v) => *tujuan = v,
        Err(pesan) => galat.push(format!("{judul}: {pesan}")),
    }
}

fn isi_wajib<T>(
    galat: &mut Vec<String>,
    judul: &str,
    hasil: Result<Option<T>, String>,
    tujuan: &mut T,
) {
    match hasil {
        Ok(Some(v)) => *tujuan = v,
        Ok(None) => {}
        Err(pesan) => galat.push(format!("{judul}: {pesan}")),
    }
}

/// Kolom yang tak ada di berkas atau kosong pada kolom wajib (nama, harga ecer, batas stok, status) mempertahankan nilai `mulai`; kolom opsional yang kosong berarti dikosongkan.
fn baca_nilai(
    lembar: &Lembar,
    baris: &[Data],
    mulai: &Nilai,
    kategori: &HashMap<String, Uuid>,
    galat: &mut Vec<String>,
) -> Nilai {
    let mut n = mulai.clone();

    if let Some(sel) = lembar.sel(baris, j::NAMA) {
        if let Some(v) = teks(sel) {
            n.nama = v;
        }
    }
    if let Some(sel) = lembar.sel(baris, j::NAMA_SEO) {
        n.seo = teks(sel);
    }
    if let Some(sel) = lembar.sel(baris, j::KATEGORI) {
        match teks(sel) {
            None => n.kategori = None,
            Some(t) => match kategori.get(&t.to_lowercase()) {
                Some(id) => n.kategori = Some(*id),
                None => galat.push(format!("Kategori \"{t}\" tidak dikenal.")),
            },
        }
    }
    if let Some(sel) = lembar.sel(baris, j::JENIS) {
        n.jenis = teks(sel);
    }
    if let Some(sel) = lembar.sel(baris, j::GRADE) {
        n.grade = teks(sel);
    }
    if let Some(sel) = lembar.sel(baris, j::MEREK) {
        n.merek = teks(sel);
    }
    if let Some(sel) = lembar.sel(baris, j::UKURAN) {
        isi(galat, j::UKURAN, kg(sel), &mut n.ukuran);
    }
    if let Some(sel) = lembar.sel(baris, j::HARGA) {
        isi_wajib(galat, j::HARGA, uang(sel), &mut n.harga);
    }
    let opsi_uang: [(&str, &mut Option<Decimal>); 5] = [
        (j::GROSIR, &mut n.grosir),
        (j::SHOPEE, &mut n.shopee),
        (j::GROSIR_SHOPEE, &mut n.grosir_shopee),
        (j::TIKTOK, &mut n.tiktok),
        (j::GROSIR_TIKTOK, &mut n.grosir_tiktok),
    ];
    for (judul, tujuan) in opsi_uang {
        if let Some(sel) = lembar.sel(baris, judul) {
            isi(galat, judul, uang(sel), tujuan);
        }
    }
    let opsi_kg: [(&str, &mut Option<Decimal>); 3] = [
        (j::BATAS, &mut n.batas),
        (j::BATAS_SHOPEE, &mut n.batas_shopee),
        (j::BATAS_TIKTOK, &mut n.batas_tiktok),
    ];
    for (judul, tujuan) in opsi_kg {
        if let Some(sel) = lembar.sel(baris, judul) {
            isi(galat, judul, kg(sel), tujuan);
        }
    }
    if let Some(sel) = lembar.sel(baris, j::BATAS_STOK) {
        match bulat(sel) {
            Ok(Some(v)) if v < 0 => {
                galat.push(format!("{}: tidak boleh negatif.", j::BATAS_STOK));
            }
            hasil => isi_wajib(galat, j::BATAS_STOK, hasil, &mut n.batas_stok),
        }
    }
    if let Some(sel) = lembar.sel(baris, j::STATUS) {
        isi_wajib(galat, j::STATUS, status(sel), &mut n.aktif);
    }

    let batas_panjang = [
        (j::NAMA, Some(&n.nama), 200),
        (j::NAMA_SEO, n.seo.as_ref(), 255),
        (j::JENIS, n.jenis.as_ref(), 100),
        (j::GRADE, n.grade.as_ref(), 60),
        (j::MEREK, n.merek.as_ref(), 150),
    ];
    for (judul, isi, maks) in batas_panjang {
        if isi.is_some_and(|t| t.chars().count() > maks) {
            galat.push(format!("{judul}: maksimal {maks} karakter."));
        }
    }

    n
}

type Bungkus = fn(Option<Decimal>) -> Kolom;

/// Beda `lama` dan `baru` sebagai kolom yang ditulis beserta isi tampilan untuk pratinjau.
fn selisih(
    lama: &Nilai,
    baru: &Nilai,
    nama_kategori: &HashMap<Uuid, String>,
) -> Vec<(Kolom, Perubahan)> {
    let mut hasil = Vec::new();
    let mut tambah = |judul: &str, dulu: String, kini: String, kolom: Kolom| {
        hasil.push((
            kolom,
            Perubahan {
                kolom: judul.to_string(),
                lama: dulu,
                baru: kini,
            },
        ));
    };
    let nama_kat = |id: &Option<Uuid>| {
        id.and_then(|id| nama_kategori.get(&id).cloned())
            .unwrap_or_default()
    };

    if lama.nama != baru.nama {
        tambah(
            j::NAMA,
            lama.nama.clone(),
            baru.nama.clone(),
            Kolom::Nama(baru.nama.clone()),
        );
    }
    if lama.seo != baru.seo {
        tambah(
            j::NAMA_SEO,
            tampil_teks(&lama.seo),
            tampil_teks(&baru.seo),
            Kolom::NamaSeo(baru.seo.clone()),
        );
    }
    if lama.kategori != baru.kategori {
        tambah(
            j::KATEGORI,
            nama_kat(&lama.kategori),
            nama_kat(&baru.kategori),
            Kolom::Kategori(baru.kategori),
        );
    }
    if lama.jenis != baru.jenis {
        tambah(
            j::JENIS,
            tampil_teks(&lama.jenis),
            tampil_teks(&baru.jenis),
            Kolom::Jenis(baru.jenis.clone()),
        );
    }
    if lama.grade != baru.grade {
        tambah(
            j::GRADE,
            tampil_teks(&lama.grade),
            tampil_teks(&baru.grade),
            Kolom::Grade(baru.grade.clone()),
        );
    }
    if lama.merek != baru.merek {
        tambah(
            j::MEREK,
            tampil_teks(&lama.merek),
            tampil_teks(&baru.merek),
            Kolom::Merek(baru.merek.clone()),
        );
    }
    if lama.ukuran != baru.ukuran {
        tambah(
            j::UKURAN,
            tampil_angka(&lama.ukuran),
            tampil_angka(&baru.ukuran),
            Kolom::Ukuran(baru.ukuran),
        );
    }
    if lama.harga != baru.harga {
        tambah(
            j::HARGA,
            lama.harga.normalize().to_string(),
            baru.harga.normalize().to_string(),
            Kolom::Harga(baru.harga),
        );
    }
    let opsional: [(&str, Option<Decimal>, Option<Decimal>, Bungkus); 8] = [
        (j::GROSIR, lama.grosir, baru.grosir, Kolom::Grosir),
        (j::SHOPEE, lama.shopee, baru.shopee, Kolom::Shopee),
        (
            j::GROSIR_SHOPEE,
            lama.grosir_shopee,
            baru.grosir_shopee,
            Kolom::GrosirShopee,
        ),
        (j::TIKTOK, lama.tiktok, baru.tiktok, Kolom::Tiktok),
        (
            j::GROSIR_TIKTOK,
            lama.grosir_tiktok,
            baru.grosir_tiktok,
            Kolom::GrosirTiktok,
        ),
        (j::BATAS, lama.batas, baru.batas, Kolom::Batas),
        (
            j::BATAS_SHOPEE,
            lama.batas_shopee,
            baru.batas_shopee,
            Kolom::BatasShopee,
        ),
        (
            j::BATAS_TIKTOK,
            lama.batas_tiktok,
            baru.batas_tiktok,
            Kolom::BatasTiktok,
        ),
    ];
    for (judul, dulu, kini, bungkus) in opsional {
        if dulu != kini {
            tambah(
                judul,
                tampil_angka(&dulu),
                tampil_angka(&kini),
                bungkus(kini),
            );
        }
    }
    if lama.batas_stok != baru.batas_stok {
        tambah(
            j::BATAS_STOK,
            lama.batas_stok.to_string(),
            baru.batas_stok.to_string(),
            Kolom::BatasStok(baru.batas_stok),
        );
    }
    if lama.aktif != baru.aktif {
        tambah(
            j::STATUS,
            tampil_status(lama.aktif),
            tampil_status(baru.aktif),
            Kolom::Aktif(baru.aktif),
        );
    }

    hasil
}

fn baris_error(no: i32, label: String, galat: Vec<String>) -> Baris {
    Baris {
        no,
        label,
        status: StatusBaris::Error,
        perubahan: Vec::new(),
        galat,
        aksi: None,
    }
}

/// Kelompok utama. Baris dikenali dari kolom `id`, lalu SKU; yang tak ditemukan dibuat sebagai produk baru.
pub fn rencana_utama(
    lembar: &Lembar,
    produk: &[Product],
    kategori: &[Category],
) -> AppResult<Vec<Baris>> {
    if !lembar.ada(j::ID) && !lembar.ada(j::SKU) {
        return Err(AppError::bad_request(
            "Berkas harus punya kolom id atau sku. Unduh ekspor terbaru sebagai acuan.",
        ));
    }

    let per_id: HashMap<Uuid, &Product> = produk.iter().map(|p| (p.id, p)).collect();
    let per_sku: HashMap<String, &Product> = produk
        .iter()
        .filter_map(|p| p.sku.as_ref().map(|s| (s.to_lowercase(), p)))
        .collect();
    let id_kategori: HashMap<String, Uuid> = kategori
        .iter()
        .map(|k| (k.name.trim().to_lowercase(), k.id))
        .collect();
    let nama_kategori: HashMap<Uuid, String> =
        kategori.iter().map(|k| (k.id, k.name.clone())).collect();

    let mut sudah: HashSet<Uuid> = HashSet::new();
    let mut hasil = Vec::new();

    for (i, baris) in lembar.baris.iter().enumerate() {
        if baris_kosong(baris) {
            continue;
        }
        let no = lembar.awal + i as i32;
        let id_teks = lembar.sel(baris, j::ID).and_then(teks);
        let sku_teks = lembar.sel(baris, j::SKU).and_then(teks);
        let label = sku_teks
            .clone()
            .or_else(|| lembar.sel(baris, j::NAMA).and_then(teks))
            .unwrap_or_else(|| format!("baris {no}"));

        let target: Option<&Product> = match &id_teks {
            Some(t) => match Uuid::parse_str(t)
                .ok()
                .and_then(|u| per_id.get(&u).copied())
            {
                Some(p) => Some(p),
                None => {
                    hasil.push(baris_error(
                        no,
                        label,
                        vec!["Kolom id tidak cocok dengan produk mana pun. Kosongkan id untuk produk baru.".to_string()],
                    ));
                    continue;
                }
            },
            None => sku_teks
                .as_ref()
                .and_then(|s| per_sku.get(&s.to_lowercase()).copied()),
        };

        let mut galat = Vec::new();
        if let Some(p) = target {
            if !sudah.insert(p.id) {
                galat.push("Produk ini muncul lebih dari sekali di berkas.".to_string());
            }
        }

        let mulai = target.map_or_else(Nilai::awal, Nilai::dari);
        let nilai = baca_nilai(lembar, baris, &mulai, &id_kategori, &mut galat);
        if target.is_none() {
            if nilai.nama.is_empty() {
                galat.push("Nama wajib diisi untuk produk baru.".to_string());
            }
            if lembar.sel(baris, j::HARGA).and_then(teks).is_none() {
                galat.push("Harga ecer walk in wajib diisi untuk produk baru.".to_string());
            }
        }
        if !galat.is_empty() {
            hasil.push(baris_error(no, label, galat));
            continue;
        }

        let mut ubah = selisih(&mulai, &nilai, &nama_kategori);
        if let (Some(p), Some(sku)) = (target, &sku_teks) {
            if p.sku
                .as_deref()
                .is_none_or(|lama| !lama.eq_ignore_ascii_case(sku))
            {
                ubah.push((
                    Kolom::Sku(sku.clone()),
                    Perubahan {
                        kolom: j::SKU.to_string(),
                        lama: p.sku.clone().unwrap_or_default(),
                        baru: sku.clone(),
                    },
                ));
            }
        }

        let (kolom, perubahan): (Vec<Kolom>, Vec<Perubahan>) = ubah.into_iter().unzip();
        let (status, aksi) = match target {
            None => (StatusBaris::Baru, Some(Aksi::Buat(kolom))),
            Some(_) if kolom.is_empty() => (StatusBaris::Tetap, None),
            Some(p) => (StatusBaris::Edit, Some(Aksi::Ubah { id: p.id, kolom })),
        };
        hasil.push(Baris {
            no,
            label,
            status,
            perubahan,
            galat: Vec::new(),
            aksi,
        });
    }

    Ok(hasil)
}

/// Kelompok stok. Hanya batch yang sudah ada; jumlah sisa tak boleh melebihi isi kiriman.
pub fn rencana_stok(
    lembar: &Lembar,
    produk: &[Product],
    batch: &[ProductBatch],
) -> AppResult<Vec<Baris>> {
    if !lembar.ada(j::ID_BATCH) {
        return Err(AppError::bad_request(
            "Berkas harus punya kolom id batch. Unduh ekspor stok terbaru sebagai acuan.",
        ));
    }

    let per_batch: HashMap<Uuid, &ProductBatch> = batch.iter().map(|b| (b.id, b)).collect();
    let per_produk: HashMap<Uuid, &Product> = produk.iter().map(|p| (p.id, p)).collect();
    let mut sudah: HashSet<Uuid> = HashSet::new();
    let mut hasil = Vec::new();

    for (i, baris) in lembar.baris.iter().enumerate() {
        if baris_kosong(baris) {
            continue;
        }
        let no = lembar.awal + i as i32;
        let id_teks = lembar.sel(baris, j::ID_BATCH).and_then(teks);
        let label = lembar
            .sel(baris, j::SKU)
            .and_then(teks)
            .unwrap_or_else(|| format!("baris {no}"));

        let Some(b) = id_teks
            .as_deref()
            .and_then(|t| Uuid::parse_str(t).ok())
            .and_then(|u| per_batch.get(&u).copied())
        else {
            hasil.push(baris_error(
                no,
                label,
                vec!["Batch tidak ditemukan. Batch baru tidak bisa dibuat lewat berkas; catat barang masuk di halaman produk.".to_string()],
            ));
            continue;
        };
        let label = per_produk
            .get(&b.product_id)
            .and_then(|p| p.sku.clone())
            .map_or(label, |sku| match &b.batch_number {
                Some(nomor) => format!("{sku} · {nomor}"),
                None => sku,
            });

        let mut galat = Vec::new();
        if !sudah.insert(b.id) {
            galat.push("Batch ini muncul lebih dari sekali di berkas.".to_string());
        }
        let mut ubah: Vec<(KolomBatch, Perubahan)> = Vec::new();
        let mut tambah = |judul: &str, dulu: String, kini: String, kolom: KolomBatch| {
            ubah.push((
                kolom,
                Perubahan {
                    kolom: judul.to_string(),
                    lama: dulu,
                    baru: kini,
                },
            ));
        };

        if let Some(sel) = lembar.sel(baris, j::SISA) {
            match bulat(sel) {
                Err(pesan) => galat.push(format!("{}: {pesan}", j::SISA)),
                Ok(Some(v)) if v < 0 || v > b.quantity => galat.push(format!(
                    "{}: harus antara 0 dan isi kiriman ({}).",
                    j::SISA,
                    b.quantity
                )),
                Ok(Some(v)) if v != b.remaining_qty => tambah(
                    j::SISA,
                    b.remaining_qty.to_string(),
                    v.to_string(),
                    KolomBatch::Sisa(v),
                ),
                Ok(_) => {}
            }
        }
        if let Some(sel) = lembar.sel(baris, j::KEDALUWARSA) {
            match tanggal(sel) {
                Err(pesan) => galat.push(format!("{}: {pesan}", j::KEDALUWARSA)),
                Ok(v) if v != b.expiry_date => tambah(
                    j::KEDALUWARSA,
                    b.expiry_date.map(|d| d.to_string()).unwrap_or_default(),
                    v.map(|d| d.to_string()).unwrap_or_default(),
                    KolomBatch::Kedaluwarsa(v),
                ),
                Ok(_) => {}
            }
        }
        if let Some(sel) = lembar.sel(baris, j::RAK) {
            let v = teks(sel);
            if v.as_ref().is_some_and(|t| t.chars().count() > 100) {
                galat.push(format!("{}: maksimal 100 karakter.", j::RAK));
            } else if v != b.storage_location {
                tambah(
                    j::RAK,
                    tampil_teks(&b.storage_location),
                    tampil_teks(&v),
                    KolomBatch::Rak(v),
                );
            }
        }
        if let Some(sel) = lembar.sel(baris, j::HARGA_BELI) {
            match uang(sel) {
                Err(pesan) => galat.push(format!("{}: {pesan}", j::HARGA_BELI)),
                Ok(v) if v != b.purchase_price => tambah(
                    j::HARGA_BELI,
                    tampil_angka(&b.purchase_price),
                    tampil_angka(&v),
                    KolomBatch::HargaBeli(v),
                ),
                Ok(_) => {}
            }
        }

        if !galat.is_empty() {
            hasil.push(baris_error(no, label, galat));
            continue;
        }
        let (kolom, perubahan): (Vec<KolomBatch>, Vec<Perubahan>) = ubah.into_iter().unzip();
        let (status, aksi) = if kolom.is_empty() {
            (StatusBaris::Tetap, None)
        } else {
            (
                StatusBaris::Edit,
                Some(Aksi::UbahBatch {
                    produk_id: b.product_id,
                    batch_id: b.id,
                    kolom,
                }),
            )
        };
        hasil.push(Baris {
            no,
            label,
            status,
            perubahan,
            galat: Vec::new(),
            aksi,
        });
    }

    Ok(hasil)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn lembar(judul: &[&str], baris: Vec<Vec<Data>>) -> Lembar {
        Lembar {
            kolom: judul
                .iter()
                .enumerate()
                .map(|(i, t)| (t.to_string(), i))
                .collect(),
            baris,
            awal: 2,
        }
    }

    fn s(teks: &str) -> Data {
        Data::String(teks.to_string())
    }

    fn produk(sku: &str, nama: &str, harga: i64) -> Product {
        Product {
            id: Uuid::new_v4(),
            category_id: None,
            category_name: None,
            name: nama.to_string(),
            seo_name: None,
            sku: Some(sku.to_string()),
            brand_name: None,
            product_type: None,
            variant_grade: None,
            variant_size: None,
            parent_id: None,
            variant_count: 0,
            price: Decimal::from(harga),
            price_wholesale: None,
            price_shopee: None,
            price_tiktok: None,
            price_shopee_wholesale: None,
            price_tiktok_wholesale: None,
            wholesale_threshold_kg: None,
            wholesale_threshold_kg_shopee: None,
            wholesale_threshold_kg_tiktok: None,
            cost_price: None,
            stock_qty: 0,
            low_stock_threshold: 5,
            image_url: None,
            nearest_expiry: None,
            is_active: true,
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn angka_pecahan_tiga_digit_tidak_dianggap_ribuan() {
        assert_eq!(
            kg(&Data::Float(0.125)).unwrap(),
            Some("0.125".parse().unwrap())
        );
        assert_eq!(
            uang(&Data::Float(28000.5)).unwrap(),
            Some("28000.5".parse().unwrap())
        );
    }

    #[test]
    fn teks_uang_gaya_indonesia_dibaca() {
        assert_eq!(uang(&s("Rp 28.000")).unwrap(), Some(Decimal::from(28000)));
        assert!(uang(&Data::Float(-1.0)).is_err());
    }

    #[test]
    fn tanggal_dari_teks_dan_nomor_seri() {
        let tgl = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert_eq!(tanggal(&s("2026-12-31")).unwrap(), Some(tgl));
        assert_eq!(tanggal(&Data::Float(46387.0)).unwrap(), Some(tgl));
        assert!(tanggal(&s("kemarin")).is_err());
    }

    #[test]
    fn produk_lama_hanya_edit_dan_yang_tak_dikenal_baru() {
        let ada = produk("AYM-1KG", "Dada", 40000);
        let l = lembar(
            &[j::SKU, j::NAMA, j::HARGA],
            vec![
                vec![s("AYM-1KG"), s("Dada"), Data::Float(40000.0)],
                vec![s("aym-1kg"), s("Dada Fillet"), Data::Float(42000.0)],
                vec![s("BARU-1"), s("Sayap"), Data::Float(30000.0)],
            ],
        );
        let r = rencana_utama(&l, &[ada], &[]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Tetap);
        // Baris kedua menunjuk produk yang sama dengan baris pertama.
        assert_eq!(r[1].status, StatusBaris::Error);
        assert_eq!(r[2].status, StatusBaris::Baru);
    }

    #[test]
    fn perubahan_harga_dan_nama_tercatat_lama_ke_baru() {
        let ada = produk("AYM-1KG", "Dada", 40000);
        let l = lembar(
            &[j::SKU, j::NAMA, j::HARGA],
            vec![vec![s("AYM-1KG"), s("Dada Fillet"), Data::Float(42000.0)]],
        );
        let r = rencana_utama(&l, &[ada], &[]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Edit);
        let nama = r[0].perubahan.iter().find(|p| p.kolom == j::NAMA).unwrap();
        assert_eq!(
            (nama.lama.as_str(), nama.baru.as_str()),
            ("Dada", "Dada Fillet")
        );
        assert_eq!(r[0].perubahan.len(), 2);
    }

    #[test]
    fn kolom_opsional_kosong_mengosongkan_dan_kolom_absen_tidak_disentuh() {
        let mut ada = produk("AYM-1KG", "Dada", 40000);
        ada.price_shopee = Some(Decimal::from(45000));
        ada.price_tiktok = Some(Decimal::from(46000));
        let l = lembar(&[j::SKU, j::SHOPEE], vec![vec![s("AYM-1KG"), Data::Empty]]);
        let r = rencana_utama(&l, &[ada], &[]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Edit);
        assert_eq!(r[0].perubahan.len(), 1);
        assert_eq!(r[0].perubahan[0].kolom, j::SHOPEE);
    }

    #[test]
    fn produk_baru_wajib_nama_dan_harga_dan_kategori_harus_dikenal() {
        let l = lembar(
            &[j::SKU, j::NAMA, j::KATEGORI],
            vec![vec![Data::Empty, s("Sayap"), s("Tak Ada")]],
        );
        let r = rencana_utama(&l, &[], &[]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Error);
        assert_eq!(r[0].galat.len(), 2);
    }

    #[test]
    fn id_asing_ditolak_bukan_dibuat_baru() {
        let l = lembar(
            &[j::ID, j::NAMA, j::HARGA],
            vec![vec![
                s(&Uuid::new_v4().to_string()),
                s("X"),
                Data::Float(1.0),
            ]],
        );
        let r = rencana_utama(&l, &[], &[]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Error);
    }

    #[test]
    fn berkas_tanpa_kolom_kunci_ditolak() {
        let l = lembar(&[j::NAMA], vec![]);
        assert!(rencana_utama(&l, &[], &[]).is_err());
        assert!(rencana_stok(&l, &[], &[]).is_err());
    }

    fn batch(produk_id: Uuid, isi: i32, sisa: i32) -> ProductBatch {
        ProductBatch {
            id: Uuid::new_v4(),
            product_id: produk_id,
            batch_number: Some("B1".to_string()),
            purchase_price: None,
            quantity: isi,
            remaining_qty: sisa,
            expiry_date: None,
            storage_location: None,
            received_at: Utc::now(),
            created_by: None,
        }
    }

    #[test]
    fn sisa_stok_di_atas_isi_kiriman_ditolak_dan_selisih_dicatat() {
        let p = produk("AYM-1KG", "Dada", 40000);
        let b = batch(p.id, 10, 6);
        let id = b.id.to_string();
        let l = lembar(
            &[j::ID_BATCH, j::SISA],
            vec![
                vec![s(&id), Data::Float(11.0)],
                vec![s(&Uuid::new_v4().to_string()), Data::Float(1.0)],
            ],
        );
        let r = rencana_stok(&l, std::slice::from_ref(&p), std::slice::from_ref(&b)).unwrap();
        assert_eq!(r[0].status, StatusBaris::Error);
        assert_eq!(r[1].status, StatusBaris::Error);

        let l = lembar(
            &[j::ID_BATCH, j::SISA],
            vec![vec![s(&id), Data::Float(8.0)]],
        );
        let r = rencana_stok(&l, &[p], &[b]).unwrap();
        assert_eq!(r[0].status, StatusBaris::Edit);
        assert_eq!(r[0].perubahan[0].lama, "6");
        assert_eq!(r[0].perubahan[0].baru, "8");
    }
}
