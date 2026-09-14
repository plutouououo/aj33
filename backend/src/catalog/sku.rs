//! Perakitan SKU otomatis dari atribut produk.
//!
//! Bentuknya `[Merek] - [Jenis Produk] - [Warna] - [Ukuran]`. SKU tidak lagi
//! diketik manual: SKU yang diketik manusia cepat menyimpang -- spasi
//! berlebih, huruf besar-kecil campur, urutan bagian yang berbeda antar
//! pegawai -- dan begitu menyimpang, dua barang yang sama tidak lagi bisa
//! dikenali sebagai satu.
//!
//! Modul ini murni perhitungan teks. Penjaminan keunikan butuh database dan
//! ada di `repo::sku_unik`.

/// Panjang maksimum `products.sku`. Hasil rakitan dipotong lebih pendek dari
/// ini supaya masih tersisa ruang untuk akhiran pembeda (" - 2") yang
/// dipasang saat SKU-nya ternyata sudah dipakai.
pub const SKU_MAKS: usize = 100;
const BASIS_MAKS: usize = 90;

const PEMISAH: &str = " - ";

/// Merakit SKU dari bagian-bagiannya. Bagian yang kosong dilewati, jadi
/// produk tanpa warna dan ukuran tetap dapat SKU dari merek dan jenisnya.
///
/// Mengembalikan `None` kalau tidak ada satu pun bagian yang terisi --
/// pemanggil yang memutuskan apa gantinya.
pub fn rakit(bagian: &[Option<&str>]) -> Option<String> {
    let segmen: Vec<String> = bagian
        .iter()
        .filter_map(|b| b.map(normalkan))
        .filter(|s| !s.is_empty())
        .collect();

    if segmen.is_empty() {
        return None;
    }

    Some(potong(&segmen.join(PEMISAH), BASIS_MAKS))
}

/// Huruf besar semua dan spasi dalam dirapikan jadi satu. Dua tujuan:
/// perbedaan yang tidak berarti (huruf kecil, spasi ganda) tidak lagi
/// melahirkan SKU berbeda, dan hasilnya terbaca sama di label rak maupun
/// di layar.
fn normalkan(nilai: &str) -> String {
    nilai
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Memotong di batas karakter, bukan batas byte: memotong di tengah karakter
/// multi-byte menghasilkan `String` yang tidak valid dan membuat Rust panik.
fn potong(nilai: &str, maks: usize) -> String {
    if nilai.chars().count() <= maks {
        return nilai.to_string();
    }
    nilai.chars().take(maks).collect::<String>().trim_end().to_string()
}

/// Varian ke-n dari sebuah SKU, dipakai saat SKU hasil rakitan sudah dipakai
/// produk lain. `n` dimulai dari 2 -- yang pertama memakai SKU tanpa akhiran.
pub fn dengan_akhiran(basis: &str, n: u32) -> String {
    potong(&format!("{basis}{PEMISAH}{n}"), SKU_MAKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merakit_keempat_bagian_sesuai_urutan() {
        let hasil = rakit(&[
            Some("Fiesta"),
            Some("Ayam Fillet"),
            Some("Putih"),
            Some("1 kg"),
        ]);
        assert_eq!(hasil.as_deref(), Some("FIESTA - AYAM FILLET - PUTIH - 1 KG"));
    }

    #[test]
    fn bagian_kosong_dilewati_bukan_menyisakan_pemisah_ganda() {
        // Produk tanpa warna dan ukuran tetap harus dapat SKU yang bersih,
        // bukan "FIESTA - AYAM FILLET -  - ".
        let hasil = rakit(&[Some("Fiesta"), Some("Ayam Fillet"), Some("  "), None]);
        assert_eq!(hasil.as_deref(), Some("FIESTA - AYAM FILLET"));
    }

    #[test]
    fn tanpa_satu_pun_bagian_tidak_menghasilkan_sku() {
        assert_eq!(rakit(&[None, Some(""), Some("   ")]), None);
    }

    #[test]
    fn huruf_dan_spasi_dinormalkan() {
        // "fiesta" dan "FIESTA  " harus bermuara ke SKU yang sama, kalau
        // tidak keduanya lolos sebagai dua produk berbeda.
        let a = rakit(&[Some("fiesta"), Some("ayam  fillet")]);
        let b = rakit(&[Some("  FIESTA"), Some("AYAM FILLET  ")]);
        assert_eq!(a, b);
    }

    #[test]
    fn hasil_rakitan_selalu_muat_di_kolom_sku() {
        let panjang = "x".repeat(150);
        let hasil = rakit(&[Some(&panjang), Some(&panjang)]).unwrap();
        assert!(hasil.chars().count() <= BASIS_MAKS);
        // Masih tersisa ruang untuk akhiran pembeda.
        assert!(dengan_akhiran(&hasil, 999).chars().count() <= SKU_MAKS);
    }

    #[test]
    fn memotong_di_batas_karakter_bukan_byte() {
        // Tanpa ini, memotong tepat di tengah karakter multi-byte membuat
        // program panik, bukan sekadar menghasilkan teks yang aneh.
        let panjang = "é".repeat(150);
        let hasil = rakit(&[Some(&panjang)]).unwrap();
        assert!(hasil.chars().count() <= BASIS_MAKS);
    }

    #[test]
    fn akhiran_pembeda_memakai_pemisah_yang_sama() {
        assert_eq!(dengan_akhiran("FIESTA - AYAM", 2), "FIESTA - AYAM - 2");
    }
}
