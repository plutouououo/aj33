//! Membaca .xlsx (`calamine`) atau .csv (`csv`) jadi header dan baris teks mentah; hanya sheet pertama xlsx (tak ada multi-sheet).

use crate::error::{AppError, AppResult};
use calamine::{Data, Reader, Xlsx};
use std::io::Cursor;

pub fn baca_xlsx(bytes: &[u8]) -> AppResult<(Vec<String>, Vec<Vec<String>>)> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> = Xlsx::new(cursor)
        .map_err(|err| AppError::bad_request(format!("Berkas xlsx tidak bisa dibaca: {err}")))?;

    let nama_sheet = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| AppError::bad_request("Berkas xlsx tidak punya sheet."))?;

    let range = workbook
        .worksheet_range(&nama_sheet)
        .map_err(|err| AppError::bad_request(format!("Sheet pertama tidak bisa dibaca: {err}")))?;

    let mut baris_iter = range.rows();
    let header: Vec<String> = match baris_iter.next() {
        Some(baris) => baris.iter().map(sel_ke_teks).collect(),
        None => return Ok((Vec::new(), Vec::new())),
    };

    let baris: Vec<Vec<String>> = baris_iter
        .map(|baris| baris.iter().map(sel_ke_teks).collect())
        .collect();

    Ok((header, baris))
}

/// Sel angka bulat (mis. 204795) diformat tanpa ".0" agar tak tersimpan "204795.0" dan `parse_money` tak menebak; selain itu `Display` bawaan `calamine::Data`.
fn sel_ke_teks(sel: &Data) -> String {
    match sel {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => (*f as i64).to_string(),
        lain => lain.to_string(),
    }
}

pub fn baca_csv(bytes: &[u8]) -> AppResult<(Vec<String>, Vec<Vec<String>>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(bytes);

    let header: Vec<String> = rdr
        .headers()
        .map_err(|err| AppError::bad_request(format!("Header CSV tidak bisa dibaca: {err}")))?
        .iter()
        .map(str::to_string)
        .collect();

    let mut baris = Vec::new();
    for hasil in rdr.records() {
        let rec = hasil
            .map_err(|err| AppError::bad_request(format!("Baris CSV tidak bisa dibaca: {err}")))?;
        baris.push(rec.iter().map(str::to_string).collect());
    }

    Ok((header, baris))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_header_dan_baris_terbaca() {
        let bytes = b"nama,harga\nMug Keramik,\"Rp 25.000\"\nGelas,15000\n";
        let (header, baris) = baca_csv(bytes).unwrap();
        assert_eq!(header, vec!["nama", "harga"]);
        assert_eq!(baris.len(), 2);
        assert_eq!(baris[0], vec!["Mug Keramik", "Rp 25.000"]);
        assert_eq!(baris[1], vec!["Gelas", "15000"]);
    }

    #[test]
    fn xlsx_fixture_tidak_menghasilkan_pecahan_nol_pada_angka_bulat() {
        let bytes = include_bytes!("../../tests/fixtures/impor_contoh.xlsx");
        let (header, baris) = baca_xlsx(bytes).unwrap();
        assert!(!header.is_empty());

        for baris in &baris {
            for sel in baris {
                assert!(
                    !sel.ends_with(".0"),
                    "sel {sel:?} seharusnya tidak berakhir dengan .0"
                );
            }
        }
    }

    #[test]
    fn xlsx_fixture_memuat_sku_dobel_dan_kategori_tak_dikenal() {
        let bytes = include_bytes!("../../tests/fixtures/impor_contoh.xlsx");
        let (header, baris) = baca_xlsx(bytes).unwrap();

        let idx_sku = header
            .iter()
            .position(|h| h.eq_ignore_ascii_case("sku"))
            .expect("fixture harus punya kolom sku");

        let mut sku_terlihat = std::collections::HashSet::new();
        let mut ada_dobel = false;
        for baris in &baris {
            let sku = baris[idx_sku].trim().to_lowercase();
            if !sku.is_empty() && !sku_terlihat.insert(sku) {
                ada_dobel = true;
            }
        }
        assert!(ada_dobel, "fixture harus memuat SKU dobel");
    }
}
