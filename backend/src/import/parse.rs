//! Logika murni parser dan validasi baris impor tanpa SQL dan I/O, dites tanpa database (modul `tests`).

use rust_decimal::Decimal;
use serde::Serialize;

pub const BATAS_BARIS: usize = 5000;
pub const BATAS_BYTE: usize = 10 * 1024 * 1024;
pub const EKSTENSI_DIIZINKAN: [&str; 2] = ["xlsx", "csv"];

/// Toleransi selisih harga berkas vs rumus modal/margin sebelum WARN: sekadar pembulatan, bukan penyimpangan.
const TOLERANSI_HARGA: Decimal = Decimal::from_parts(1, 0, 0, false, 0);

/// Field kanonik yang dikenali dari header spreadsheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Sku,
    Category,
    Brand,
    ProductType,
    VariantGrade,
    VariantSize,
    LowestPrice,
    Cost,
    Margin,
    Stock,
    Published,
}

/// Header (Indonesia/Inggris) ke field kanonik; kolom tak dikenal `None` dan diabaikan pemanggil, bukan ditolak, karena berkas boleh punya kolom tambahan.
pub fn map_header(header: &str) -> Option<Field> {
    match header.trim().to_lowercase().as_str() {
        "nama" | "name" => Some(Field::Name),
        "sku" | "kode" => Some(Field::Sku),
        "kategori" | "category" => Some(Field::Category),
        "merek" | "brand" => Some(Field::Brand),
        "jenis" | "product_type" => Some(Field::ProductType),
        "grade" | "variant_grade" => Some(Field::VariantGrade),
        "ukuran" | "size" | "variant_size" => Some(Field::VariantSize),
        "harga" | "lowest_price" => Some(Field::LowestPrice),
        "modal" | "cost" => Some(Field::Cost),
        "margin" | "margin_pct" => Some(Field::Margin),
        "stok" | "stock" => Some(Field::Stock),
        "terbit" | "published" => Some(Field::Published),
        _ => None,
    }
}

/// Menerima "Rp 1.234.567", "1.234.567,89", "1,234,567.00", "204795": pemisah paling kanan jadi desimal bila keduanya ada; satu jenis berulang atau grup 3 digit berarti ribuan.
pub fn parse_money(raw: &str) -> Option<Decimal> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // "Rp" di depan (tak peka huruf) dibuang dulu, posisinya dicari di teks asli agar sisanya memuat karakter aslinya.
    let tanpa_rp = {
        let lower = trimmed.to_lowercase();
        if let Some(sisa) = lower.strip_prefix("rp") {
            &trimmed[trimmed.len() - sisa.len()..]
        } else {
            trimmed
        }
    };

    let tanpa_spasi: String = tanpa_rp.chars().filter(|c| !c.is_whitespace()).collect();
    if tanpa_spasi.is_empty() {
        return None;
    }

    let mut sisa = tanpa_spasi.as_str();
    let negatif = sisa.starts_with('-');
    if negatif || sisa.starts_with('+') {
        sisa = &sisa[1..];
    }
    if sisa.is_empty() {
        return None;
    }

    let ada_koma = sisa.contains(',');
    let ada_titik = sisa.contains('.');

    let pemisah_desimal: Option<char> = if ada_koma && ada_titik {
        let posisi_koma = sisa.rfind(',').unwrap();
        let posisi_titik = sisa.rfind('.').unwrap();
        Some(if posisi_koma > posisi_titik { ',' } else { '.' })
    } else if ada_koma || ada_titik {
        let pemisah = if ada_koma { ',' } else { '.' };
        let jumlah = sisa.matches(pemisah).count();
        if jumlah > 1 {
            None
        } else {
            let posisi = sisa.find(pemisah).unwrap();
            let setelah = &sisa[posisi + 1..];
            let kelompok_ribuan = setelah.len() == 3 && setelah.bytes().all(|b| b.is_ascii_digit());
            if kelompok_ribuan {
                None
            } else {
                Some(pemisah)
            }
        }
    } else {
        None
    };

    let mut hasil = String::with_capacity(sisa.len());
    for c in sisa.chars() {
        match c {
            ',' | '.' if Some(c) == pemisah_desimal => hasil.push('.'),
            ',' | '.' => {}
            d if d.is_ascii_digit() => hasil.push(d),
            _ => return None,
        }
    }
    if hasil.is_empty() || hasil == "." {
        return None;
    }

    let mut nilai: Decimal = hasil.parse().ok()?;
    if negatif {
        nilai = -nilai;
    }
    Some(nilai)
}

/// Ukuran pack dalam kg dari teks bebas ("2 kg", "0,9", "500 gr"); tanpa satuan dianggap kg, dan nol atau negatif ditolak karena bukan isi pack.
pub fn parse_ukuran_kg(raw: &str) -> Option<Decimal> {
    let kecil = raw.trim().to_lowercase();
    let (angka, pembagi) = if let Some(sisa) = kecil.strip_suffix("kg") {
        (sisa, 1)
    } else if let Some(sisa) = kecil
        .strip_suffix("gram")
        .or_else(|| kecil.strip_suffix("gr"))
        .or_else(|| kecil.strip_suffix('g'))
    {
        (sisa, 1000)
    } else {
        (kecil.as_str(), 1)
    };

    let nilai = parse_money(angka)? / Decimal::from(pembagi);
    (nilai > Decimal::ZERO).then_some(nilai)
}

/// Buang "%" lalu `parse_money`; hasil angka persen apa adanya ("17,25%" → 17.25), bukan pecahan (pemanggil membagi 100, lihat `harga_efektif`).
pub fn parse_percent(raw: &str) -> Option<Decimal> {
    parse_money(&raw.replace('%', ""))
}

/// Kolom bilangan bulat (stok) lewat `parse_money` dulu agar pemisah ribuan terbaca, lalu dibulatkan.
pub fn parse_int(raw: &str) -> Option<i32> {
    let nilai = parse_money(raw)?.round();
    nilai.to_string().split('.').next()?.parse().ok()
}

pub fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_lowercase().as_str() {
        "ya" | "yes" | "true" | "1" | "y" => Some(true),
        "tidak" | "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub level: Level,
    pub field: String,
    pub msg: String,
}

fn error(field: &str, msg: impl Into<String>) -> Issue {
    Issue {
        level: Level::Error,
        field: field.to_string(),
        msg: msg.into(),
    }
}

fn warn(field: &str, msg: impl Into<String>) -> Issue {
    Issue {
        level: Level::Warn,
        field: field.to_string(),
        msg: msg.into(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedRow {
    pub name: Option<String>,
    pub sku: Option<String>,
    pub category_text: Option<String>,
    pub brand_text: Option<String>,
    pub product_type: Option<String>,
    pub variant_grade: Option<String>,
    pub variant_size: Option<String>,
    pub lowest_price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub margin_pct: Option<Decimal>,
    pub stock: Option<i32>,
    pub published: Option<bool>,
}

/// Harga create/update: eksplisit menang, kosong dihitung modal/(1 - margin/100); WARN bila menyimpang melebihi `TOLERANSI_HARGA`, murni informatif.
pub fn harga_efektif(
    harga: Option<Decimal>,
    modal: Option<Decimal>,
    margin: Option<Decimal>,
) -> (Option<Decimal>, Option<Issue>) {
    let seratus = Decimal::from(100);
    let dari_rumus = match (modal, margin) {
        (Some(m), Some(p)) if p < seratus => Some((m / (Decimal::ONE - p / seratus)).round_dp(2)),
        _ => None,
    };

    match (harga, dari_rumus) {
        (Some(h), Some(r)) => {
            let selisih = (h - r).abs();
            let isu = (selisih > TOLERANSI_HARGA).then(|| {
                warn(
                    "lowest_price",
                    format!(
                        "Harga di berkas ({h}) menyimpang dari hasil rumus modal/margin ({r})."
                    ),
                )
            });
            (Some(h), isu)
        }
        (Some(h), None) => (Some(h), None),
        (None, Some(r)) => (Some(r), None),
        (None, None) => (None, None),
    }
}

/// Semua pemeriksaan tanpa database; resolusi kategori (butuh query) sudah dilakukan pemanggil, `kategori_dikenal` hanya mengabarkan hasilnya (`true` juga bila tak ada `category_text`).
pub fn validate_row(row: &ParsedRow, sku_duplikat: bool, kategori_dikenal: bool) -> Vec<Issue> {
    let mut isu = Vec::new();

    match row.name.as_deref().map(str::trim) {
        None | Some("") => isu.push(error("name", "Nama produk wajib diisi.")),
        Some(n) if n.chars().count() > 220 => {
            isu.push(error("name", "Nama produk maksimal 220 karakter."))
        }
        _ => {}
    }

    if row.lowest_price.is_some_and(|h| h.is_sign_negative()) {
        isu.push(error("lowest_price", "Harga tidak boleh negatif."));
    }
    if row.cost.is_some_and(|m| m.is_sign_negative()) {
        isu.push(error("cost", "Modal tidak boleh negatif."));
    }
    if row.margin_pct.is_some_and(|p| p.is_sign_negative()) {
        isu.push(error("margin_pct", "Margin tidak boleh negatif."));
    }
    if sku_duplikat {
        isu.push(error(
            "sku",
            "SKU ini sudah dipakai baris lain di berkas yang sama.",
        ));
    }

    if row.lowest_price.is_none() {
        isu.push(warn(
            "lowest_price",
            "Harga kosong -- akan dicatat sebagai 0.",
        ));
    }
    match (row.cost.is_some(), row.margin_pct.is_some()) {
        (true, false) => isu.push(warn("margin_pct", "Modal diisi tanpa margin.")),
        (false, true) => isu.push(warn("cost", "Margin diisi tanpa modal.")),
        _ => {}
    }
    if let (_, Some(isu_rumus)) = harga_efektif(row.lowest_price, row.cost, row.margin_pct) {
        isu.push(isu_rumus);
    }

    if row
        .variant_size
        .as_deref()
        .is_some_and(|u| parse_ukuran_kg(u).is_none())
    {
        isu.push(warn(
            "variant_size",
            "Ukuran tidak terbaca sebagai angka kg (mis. \"2 kg\") -- ukuran dikosongkan.",
        ));
    }

    if row.category_text.is_some() && !kategori_dikenal {
        isu.push(warn(
            "category_text",
            "Kategori tidak dikenal -- baris tetap diproses tanpa kategori.",
        ));
    }

    isu
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- map_header ---

    #[test]
    fn map_header_mengenali_sinonim_indonesia_dan_inggris() {
        assert_eq!(map_header("Nama"), Some(Field::Name));
        assert_eq!(map_header("name"), Some(Field::Name));
        assert_eq!(map_header("SKU"), Some(Field::Sku));
        assert_eq!(map_header("kode"), Some(Field::Sku));
        assert_eq!(map_header("Kategori"), Some(Field::Category));
        assert_eq!(map_header("category"), Some(Field::Category));
        assert_eq!(map_header("Merek"), Some(Field::Brand));
        assert_eq!(map_header("brand"), Some(Field::Brand));
        assert_eq!(map_header("Jenis"), Some(Field::ProductType));
        assert_eq!(map_header("Grade"), Some(Field::VariantGrade));
        assert_eq!(map_header("Ukuran"), Some(Field::VariantSize));
        assert_eq!(map_header("size"), Some(Field::VariantSize));
        assert_eq!(map_header("Harga"), Some(Field::LowestPrice));
        assert_eq!(map_header("Modal"), Some(Field::Cost));
        assert_eq!(map_header("Margin"), Some(Field::Margin));
        assert_eq!(map_header("Stok"), Some(Field::Stock));
        assert_eq!(map_header("stock"), Some(Field::Stock));
        assert_eq!(map_header("Terbit"), Some(Field::Published));
        assert_eq!(map_header("published"), Some(Field::Published));
    }

    #[test]
    fn map_header_kolom_asing_diabaikan_bukan_ditolak() {
        assert_eq!(map_header("Catatan Internal"), None);
        assert_eq!(map_header(""), None);
    }

    #[test]
    fn map_header_tidak_peka_spasi_dan_huruf_besar() {
        assert_eq!(map_header("  NAMA  "), Some(Field::Name));
    }

    // --- parse_ukuran_kg ---

    #[test]
    fn parse_ukuran_kg_membaca_satuan_kg_dan_gram() {
        assert_eq!(parse_ukuran_kg("2 kg"), Some(Decimal::new(2, 0)));
        assert_eq!(parse_ukuran_kg("0,9KG"), Some(Decimal::new(9, 1)));
        assert_eq!(parse_ukuran_kg("500 gr"), Some(Decimal::new(5, 1)));
        assert_eq!(parse_ukuran_kg("1.5"), Some(Decimal::new(15, 1)));
    }

    #[test]
    fn parse_ukuran_kg_menolak_nol_negatif_dan_bukan_angka() {
        assert_eq!(parse_ukuran_kg("0 kg"), None);
        assert_eq!(parse_ukuran_kg("-2 kg"), None);
        assert_eq!(parse_ukuran_kg("besar"), None);
        assert_eq!(parse_ukuran_kg(""), None);
    }

    // --- parse_money ---

    #[test]
    fn parse_money_format_rp_titik_ribuan() {
        assert_eq!(parse_money("Rp 1.234.567"), Some(Decimal::new(1234567, 0)));
    }

    #[test]
    fn parse_money_format_koma_ribuan_titik_desimal() {
        assert_eq!(
            parse_money("1,234,567.00"),
            Some(Decimal::new(123456700, 2))
        );
    }

    #[test]
    fn parse_money_angka_polos() {
        assert_eq!(parse_money("204795"), Some(Decimal::new(204795, 0)));
    }

    #[test]
    fn parse_money_titik_ribuan_koma_desimal() {
        assert_eq!(
            parse_money("1.234.567,89"),
            Some(Decimal::new(123456789, 2))
        );
    }

    #[test]
    fn parse_money_satu_titik_tiga_digit_setelahnya_ribuan() {
        assert_eq!(parse_money("1.234"), Some(Decimal::new(1234, 0)));
    }

    #[test]
    fn parse_money_satu_titik_bukan_tiga_digit_adalah_desimal() {
        assert_eq!(parse_money("1.23"), Some(Decimal::new(123, 2)));
    }

    #[test]
    fn parse_money_satu_koma_muncul_lebih_dari_sekali_adalah_ribuan() {
        assert_eq!(parse_money("1,234,567"), Some(Decimal::new(1234567, 0)));
    }

    #[test]
    fn parse_money_negatif() {
        assert_eq!(parse_money("-5000"), Some(Decimal::new(-5000, 0)));
    }

    #[test]
    fn parse_money_kosong_atau_bukan_angka() {
        assert_eq!(parse_money(""), None);
        assert_eq!(parse_money("   "), None);
        assert_eq!(parse_money("abc"), None);
    }

    // --- parse_percent ---

    #[test]
    fn parse_percent_buang_tanda_persen() {
        assert_eq!(parse_percent("17,25%"), Some(Decimal::new(1725, 2)));
        assert_eq!(parse_percent("30%"), Some(Decimal::new(30, 0)));
    }

    // --- parse_int ---

    #[test]
    fn parse_int_bilangan_bulat_dan_berformat_ribuan() {
        assert_eq!(parse_int("10"), Some(10));
        assert_eq!(parse_int("1.000"), Some(1000));
        assert_eq!(parse_int(""), None);
    }

    // --- parse_bool ---

    #[test]
    fn parse_bool_variasi_benar_dan_salah() {
        for s in ["ya", "yes", "true", "1", "y", "YA", " Ya "] {
            assert_eq!(parse_bool(s), Some(true), "gagal untuk {s:?}");
        }
        for s in ["tidak", "no", "false", "0"] {
            assert_eq!(parse_bool(s), Some(false), "gagal untuk {s:?}");
        }
        assert_eq!(parse_bool("mungkin"), None);
        assert_eq!(parse_bool(""), None);
    }

    // --- harga_efektif ---

    #[test]
    fn harga_efektif_eksplisit_menang_tanpa_warn_dalam_toleransi() {
        let (harga, isu) = harga_efektif(
            Some(Decimal::new(10000, 0)),
            Some(Decimal::new(7000, 0)),
            Some(Decimal::new(30, 0)),
        );
        // modal/(1-30%) = 7000/0.7 = 10000 -- persis sama, tidak ada WARN.
        assert_eq!(harga, Some(Decimal::new(10000, 0)));
        assert!(isu.is_none());
    }

    #[test]
    fn harga_efektif_warn_saat_menyimpang_dari_rumus() {
        let (harga, isu) = harga_efektif(
            Some(Decimal::new(50000, 0)),
            Some(Decimal::new(7000, 0)),
            Some(Decimal::new(30, 0)),
        );
        assert_eq!(harga, Some(Decimal::new(50000, 0)));
        assert!(isu.is_some());
        assert_eq!(isu.unwrap().level, Level::Warn);
    }

    #[test]
    fn harga_efektif_dihitung_dari_rumus_saat_harga_kosong() {
        let (harga, isu) =
            harga_efektif(None, Some(Decimal::new(7000, 0)), Some(Decimal::new(30, 0)));
        assert_eq!(harga, Some(Decimal::new(10000, 0)));
        assert!(isu.is_none());
    }

    #[test]
    fn harga_efektif_kosong_semua_menghasilkan_none() {
        assert_eq!(harga_efektif(None, None, None), (None, None));
    }

    // --- validate_row ---

    fn baris_valid() -> ParsedRow {
        ParsedRow {
            name: Some("Produk Uji".into()),
            lowest_price: Some(Decimal::new(10000, 0)),
            ..Default::default()
        }
    }

    #[test]
    fn validate_row_nama_kosong_adalah_error() {
        let row = ParsedRow {
            name: None,
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        assert!(isu
            .iter()
            .any(|i| i.level == Level::Error && i.field == "name"));
    }

    #[test]
    fn validate_row_nama_lebih_dari_220_karakter_adalah_error() {
        let row = ParsedRow {
            name: Some("a".repeat(221)),
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        assert!(isu
            .iter()
            .any(|i| i.level == Level::Error && i.field == "name"));
    }

    #[test]
    fn validate_row_harga_modal_margin_negatif_adalah_error() {
        let row = ParsedRow {
            lowest_price: Some(Decimal::new(-1, 0)),
            cost: Some(Decimal::new(-1, 0)),
            margin_pct: Some(Decimal::new(-1, 0)),
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        assert_eq!(
            isu.iter()
                .filter(|i| i.level == Level::Error)
                .filter(|i| ["lowest_price", "cost", "margin_pct"].contains(&i.field.as_str()))
                .count(),
            3
        );
    }

    #[test]
    fn validate_row_sku_duplikat_adalah_error() {
        let isu = validate_row(&baris_valid(), true, true);
        assert!(isu
            .iter()
            .any(|i| i.level == Level::Error && i.field == "sku"));
    }

    #[test]
    fn validate_row_harga_kosong_adalah_warn_bukan_error() {
        let row = ParsedRow {
            lowest_price: None,
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        let ditemukan = isu
            .iter()
            .find(|i| i.field == "lowest_price" && i.msg.contains("kosong"))
            .expect("harus ada WARN harga kosong");
        assert_eq!(ditemukan.level, Level::Warn);
    }

    #[test]
    fn validate_row_modal_tanpa_margin_adalah_warn() {
        let row = ParsedRow {
            cost: Some(Decimal::new(5000, 0)),
            margin_pct: None,
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        assert!(isu
            .iter()
            .any(|i| i.level == Level::Warn && i.field == "margin_pct"));
    }

    #[test]
    fn validate_row_margin_tanpa_modal_adalah_warn() {
        let row = ParsedRow {
            cost: None,
            margin_pct: Some(Decimal::new(30, 0)),
            ..baris_valid()
        };
        let isu = validate_row(&row, false, true);
        assert!(isu
            .iter()
            .any(|i| i.level == Level::Warn && i.field == "cost"));
    }

    #[test]
    fn validate_row_kategori_tak_dikenal_adalah_warn_bukan_error() {
        let row = ParsedRow {
            category_text: Some("Kategori Aneh".into()),
            ..baris_valid()
        };
        let isu = validate_row(&row, false, false);
        let ditemukan = isu
            .iter()
            .find(|i| i.field == "category_text")
            .expect("harus ada WARN kategori");
        assert_eq!(ditemukan.level, Level::Warn);
    }

    #[test]
    fn validate_row_baris_bersih_tidak_ada_error() {
        let isu = validate_row(&baris_valid(), false, true);
        assert!(isu.iter().all(|i| i.level != Level::Error));
    }
}
