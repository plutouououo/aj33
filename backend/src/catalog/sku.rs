//! Perakitan SKU otomatis dari atribut produk.
//!
//! Bentuknya `[Jenis Produk + Grade]-[Merek]-[Ukuran]`, mis. `CBSB-AFC-2KG`.
//!
//! Aturan ini bukan karangan baru: toko sudah memakainya bertahun-tahun,
//! diketik tangan di nama produk. `CBSB` adalah **C**eker **B**ersih
//! **S**uper **B**esar, `HJA` adalah **H**ati **J**antung **A**mpela. Yang
//! berubah hanya siapa yang mengetiknya. Karena kodenya sama persis dengan
//! yang sudah dihafal pegawai, tidak ada yang perlu belajar ulang -- dan
//! pencarian dengan kode lama tetap menemukan barangnya.
//!
//! SKU tidak pernah diketik manual. SKU yang diketik manusia cepat menyimpang
//! -- spasi berlebih, huruf besar-kecil campur, urutan bagian yang berbeda
//! antar pegawai -- dan begitu menyimpang, dua barang yang sama tidak lagi
//! bisa dikenali sebagai satu.
//!
//! Modul ini murni perhitungan teks. Penjaminan keunikan butuh database dan
//! ada di `repo::sku_unik`.

/// Panjang maksimum `products.sku`. Hasil rakitan dipotong lebih pendek dari
/// ini supaya masih tersisa ruang untuk akhiran pembeda ("-2") yang dipasang
/// saat SKU-nya ternyata sudah dipakai.
pub const SKU_MAKS: usize = 100;
const BASIS_MAKS: usize = 90;

const PEMISAH: &str = "-";

/// Merakit SKU dari atribut produk.
///
/// Jenis produk dan grade dilebur jadi satu kode inisial; merek dipendekkan
/// jadi tiga huruf; ukuran dibawa apa adanya. Bagian yang kosong dilewati,
/// jadi barang tanpa grade dan tanpa ukuran tetap dapat SKU.
///
/// Mengembalikan `None` kalau tidak ada satu pun bagian yang terisi --
/// pemanggil yang memutuskan apa gantinya.
pub fn rakit(
    jenis_produk: Option<&str>,
    grade: Option<&str>,
    merek: Option<&str>,
    ukuran: Option<&str>,
) -> Option<String> {
    let mut segmen: Vec<String> = Vec::new();

    // Jenis dan grade menyatu tanpa pemisah: itulah yang menghasilkan CBSB
    // dari "Ceker Bersih" + "Super Besar".
    let kode = format!(
        "{}{}",
        jenis_produk.map(inisial).unwrap_or_default(),
        grade.map(inisial).unwrap_or_default()
    );
    if !kode.is_empty() {
        segmen.push(kode);
    }

    if let Some(merek) = merek.map(singkatan_merek) {
        if !merek.is_empty() {
            segmen.push(merek);
        }
    }

    if let Some(ukuran) = ukuran.map(rapatkan) {
        if !ukuran.is_empty() {
            segmen.push(ukuran);
        }
    }

    if segmen.is_empty() {
        return None;
    }

    Some(potong(&segmen.join(PEMISAH), BASIS_MAKS))
}

/// Kode inisial satu bagian.
///
/// Bagian yang **mengandung angka** dibawa utuh, bukan diambil inisialnya:
/// grade "SP 08" yang menyusut jadi "S0" berhenti bisa dibaca, sedangkan
/// "SP08" masih jelas menunjuk kelas ukuran yang mana. Bagian tanpa angka
/// diambil huruf pertama tiap katanya -- itu yang melahirkan CBSB dan HJA.
fn inisial(bagian: &str) -> String {
    if bagian.chars().any(|c| c.is_ascii_digit()) {
        return rapatkan(bagian);
    }

    bagian
        .split_whitespace()
        .filter_map(|kata| kata.chars().find(|c| c.is_alphanumeric()))
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Merek dipendekkan jadi tiga huruf, dibagi rata menurut jumlah katanya:
/// satu kata mengambil tiga huruf pertama (afco -> AFC), dua kata mengambil
/// dua huruf dari yang pertama dan satu dari yang kedua (best chicken ->
/// BEC), tiga kata atau lebih mengambil satu huruf dari masing-masing tiga
/// kata pertama.
fn singkatan_merek(merek: &str) -> String {
    let kata: Vec<&str> = merek.split_whitespace().collect();

    let ambil = |kata: &str, n: usize| -> String {
        kata.chars()
            .filter(|c| c.is_alphanumeric())
            .take(n)
            .flat_map(|c| c.to_uppercase())
            .collect()
    };

    match kata.as_slice() {
        [] => String::new(),
        [satu] => ambil(satu, 3),
        [satu, dua] => format!("{}{}", ambil(satu, 2), ambil(dua, 1)),
        [satu, dua, tiga, ..] => {
            format!("{}{}{}", ambil(satu, 1), ambil(dua, 1), ambil(tiga, 1))
        }
    }
}

/// Huruf besar semua, spasi dan tanda baca dibuang. Dipakai untuk bagian yang
/// dibawa utuh ke dalam SKU: "2 kg" jadi "2KG", "SP 08" jadi "SP08".
fn rapatkan(nilai: &str) -> String {
    nilai
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Memotong di batas karakter, bukan batas byte: memotong di tengah karakter
/// multi-byte menghasilkan `String` yang tidak valid dan membuat Rust panik.
fn potong(nilai: &str, maks: usize) -> String {
    if nilai.chars().count() <= maks {
        return nilai.to_string();
    }
    nilai.chars().take(maks).collect::<String>()
}

/// Varian ke-n dari sebuah SKU, dipakai saat SKU hasil rakitan sudah dipakai
/// produk lain. `n` dimulai dari 2 -- yang pertama memakai SKU tanpa akhiran.
pub fn dengan_akhiran(basis: &str, n: u32) -> String {
    potong(&format!("{basis}{PEMISAH}{n}"), SKU_MAKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kode-kode ini sudah dipakai toko sebelum sistem ini ada, diketik
    /// tangan di nama produk. Aturan perakitan harus menghasilkan kode yang
    /// SAMA -- kalau berbeda, pegawai kehilangan hafalannya dan pencarian
    /// dengan kode lama berhenti menemukan barangnya.
    #[test]
    fn menghasilkan_kode_yang_sudah_dipakai_toko() {
        // CBSB: ceker bersih super besar afco
        assert_eq!(
            rakit(
                Some("Ceker Bersih"),
                Some("Super Besar"),
                Some("afco"),
                None
            )
            .as_deref(),
            Some("CBSB-AFC")
        );

        // Hati Jantung Ampela _HJA_ afco
        assert_eq!(
            rakit(Some("Hati Jantung Ampela"), None, Some("afco"), None).as_deref(),
            Some("HJA-AFC")
        );
    }

    #[test]
    fn grade_berangka_dibawa_utuh_bukan_disingkat() {
        // "SP 08" yang jadi "S0" berhenti bisa dibaca; yang dicari orang
        // adalah kelas ukurannya, dan itu ada di angkanya.
        assert_eq!(
            rakit(Some("Ayam Utuh"), Some("SP 08"), Some("afco"), Some("2 kg")).as_deref(),
            Some("AUSP08-AFC-2KG")
        );
    }

    #[test]
    fn merek_dipendekkan_menurut_jumlah_katanya() {
        assert_eq!(singkatan_merek("afco"), "AFC");
        assert_eq!(singkatan_merek("best chicken"), "BEC");
        assert_eq!(singkatan_merek("OK CHICK"), "OKC");
        // Merek di luar daftar tetap dapat kode tiga huruf.
        assert_eq!(singkatan_merek("bsb whole best chicken"), "BWB");
        assert_eq!(singkatan_merek(""), "");
    }

    #[test]
    fn bagian_kosong_dilewati_bukan_menyisakan_pemisah_ganda() {
        // Barang tanpa grade dan tanpa ukuran tetap harus dapat SKU bersih,
        // bukan "HJA--AFC-".
        assert_eq!(
            rakit(Some("Hati"), Some("   "), Some("afco"), Some("")).as_deref(),
            Some("H-AFC")
        );
    }

    #[test]
    fn tanpa_satu_pun_bagian_tidak_menghasilkan_sku() {
        assert_eq!(rakit(None, Some(""), Some("   "), None), None);
    }

    #[test]
    fn huruf_dan_spasi_dinormalkan() {
        // "afco" dan "  AFCO " harus bermuara ke SKU yang sama, kalau tidak
        // keduanya lolos sebagai dua produk berbeda.
        let a = rakit(
            Some("ceker bersih"),
            Some("super besar"),
            Some("afco"),
            None,
        );
        let b = rakit(
            Some("  Ceker  Bersih"),
            Some("SUPER BESAR "),
            Some(" AFCO"),
            None,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn hasil_rakitan_selalu_muat_di_kolom_sku() {
        let panjang = "kata ".repeat(200);
        let hasil = rakit(
            Some(&panjang),
            Some(&panjang),
            Some(&panjang),
            Some(&panjang),
        )
        .unwrap();
        assert!(hasil.chars().count() <= BASIS_MAKS);
        // Masih tersisa ruang untuk akhiran pembeda.
        assert!(dengan_akhiran(&hasil, 999).chars().count() <= SKU_MAKS);
    }

    #[test]
    fn memotong_di_batas_karakter_bukan_byte() {
        // Tanpa ini, memotong tepat di tengah karakter multi-byte membuat
        // program panik, bukan sekadar menghasilkan teks yang aneh.
        let panjang = "é".repeat(150);
        let hasil = rakit(Some(&panjang), None, None, None).unwrap();
        assert!(hasil.chars().count() <= BASIS_MAKS);
    }

    #[test]
    fn akhiran_pembeda_memakai_pemisah_yang_sama() {
        assert_eq!(dengan_akhiran("CBSB-AFC", 2), "CBSB-AFC-2");
    }
}
