//! Perakitan SKU otomatis dari atribut produk.
//!
//! Bentuk kode induk adalah `[Jenis Produk + Grade]-[Merek]-[Ukuran]`, mis.
//! `CBSB-AFC-2KG`. Varian menambahkan sumbu variannya di belakang kode
//! induknya, jadi seluruh varian satu produk berbagi satu awalan: `CB-AFC`
//! melahirkan `CB-AFC-SB-2KG` dan `CB-AFC-SB-5KG`. Mencari "CB-AFC" menemukan
//! semuanya sekaligus.
//!
//! Aturan ini bukan karangan baru: toko sudah memakainya bertahun-tahun,
//! diketik tangan di nama produk. `CBSB` adalah **C**eker **B**ersih
//! **S**uper **B**esar, `HJA` adalah **H**ati **J**antung **A**mpela. Yang
//! berubah hanya siapa yang mengetiknya. Karena kodenya sama persis dengan
//! yang sudah dihafal pegawai, tidak ada yang perlu belajar ulang -- dan
//! pencarian dengan kode lama tetap menemukan barangnya.
//!
//! SKU tidak diketik saat produk dibuat. SKU yang diketik manusia cepat
//! menyimpang -- spasi berlebih, huruf besar-kecil campur, urutan bagian yang
//! berbeda antar pegawai -- dan begitu menyimpang, dua barang yang sama tidak
//! lagi bisa dikenali sebagai satu.
//!
//! # SKU dibekukan setelah dirakit
//!
//! Sekali terpasang, SKU tidak pernah dirakit ulang -- termasuk saat atribut
//! pembentuknya disunting. SKU yang ikut berubah memutus tiga hal sekaligus:
//! label yang sudah dicetak dan ditempel di pack, listing marketplace yang
//! sudah memetakan SKU lama, dan hafalan pegawai yang mencari dengan kode
//! lama. Atribut adalah kebenaran barangnya; SKU cuma namanya, dan nama tidak
//! ikut berubah tiap kali keterangannya diperbaiki.
//!
//! Karena beku, harus ada jalan koreksi untuk salah ketik di awal, dan itu
//! `normalkan`: SKU boleh diperbaiki manual selama produknya belum bergerak
//! (penjagaannya `repo::penahan_hapus`, sama persis dengan larangan hapus).
//! Setelah bergerak, yang tersisa adalah menonaktifkan produknya.
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

/// Bagian SKU yang membedakan satu varian dari saudara-saudaranya, dirakit
/// dari sumbu varian: grade dan ukuran. Dipasang di belakang SKU induknya,
/// sehingga seluruh varian satu produk berbagi awalan yang sama dan satu
/// pencarian menemukan semuanya.
///
/// Merek dan jenis produk sengaja tidak ikut: keduanya milik induk, dan
/// mengulangnya di tiap varian hanya memanjangkan kode tanpa membedakan apa
/// pun.
///
/// Mengembalikan `None` kalau kedua sumbunya kosong -- varian yang tidak
/// punya pembeda untuk ditulis. Pemanggil yang memutuskan apa gantinya.
pub fn varian(grade: Option<&str>, ukuran: Option<&str>) -> Option<String> {
    let mut segmen: Vec<String> = Vec::new();

    if let Some(grade) = grade.map(inisial) {
        if !grade.is_empty() {
            segmen.push(grade);
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

    Some(segmen.join(PEMISAH))
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

/// Membersihkan SKU yang diketik manusia menjadi bentuk yang sama dengan
/// hasil rakitan: huruf besar semua, dan tiap rentetan karakter bukan
/// huruf-angka -- spasi, garis bawah, garis miring, pemisah berulang --
/// menyusut jadi satu pemisah. Tanpa ini "cbsb afc / 2kg" dan "CBSB-AFC-2KG"
/// lolos sebagai dua SKU berbeda untuk barang yang sama, yang justru
/// penyimpangan yang dihindari dengan merakit SKU otomatis.
///
/// Dipakai hanya di jalur koreksi; produk baru tidak pernah mengetik SKU.
///
/// Mengembalikan `None` kalau tidak tersisa satu pun huruf atau angka.
pub fn normalkan(kode: &str) -> Option<String> {
    let mut hasil = String::new();
    // Pemisah baru benar-benar ditulis saat ada isi sesudahnya, jadi pemisah
    // di ujung depan dan ujung belakang tidak pernah ikut tersimpan.
    let mut tertunda = false;

    for c in kode.chars() {
        if c.is_alphanumeric() {
            if tertunda && !hasil.is_empty() {
                hasil.push_str(PEMISAH);
            }
            tertunda = false;
            hasil.extend(c.to_uppercase());
        } else {
            tertunda = true;
        }
    }

    if hasil.is_empty() {
        return None;
    }

    Some(potong(&hasil, BASIS_MAKS))
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
    fn varian_dirakit_dari_sumbu_variannya_saja() {
        // Grade tanpa angka diambil inisialnya, ukuran dibawa utuh.
        assert_eq!(
            varian(Some("Super Besar"), Some("2 kg")).as_deref(),
            Some("SB-2KG")
        );
        // Grade berangka tetap dibawa utuh, sama seperti di kode induk.
        assert_eq!(varian(Some("SP 08"), None).as_deref(), Some("SP08"));
        assert_eq!(varian(None, Some("5 kg")).as_deref(), Some("5KG"));
    }

    #[test]
    fn varian_tanpa_sumbu_tidak_menghasilkan_pembeda() {
        // Bukan string kosong: string kosong akan menempel ke SKU induk
        // sebagai pemisah menggantung ("CB-AFC-").
        assert_eq!(varian(None, None), None);
        assert_eq!(varian(Some("  "), Some("")), None);
    }

    #[test]
    fn seluruh_varian_satu_induk_berbagi_awalan() {
        // Inilah sebabnya varian memakai SKU induk sebagai awalan: satu
        // pencarian "CB-AFC" harus menemukan seluruh ukurannya.
        let induk = rakit(Some("Ceker Bersih"), None, Some("afco"), None).unwrap();
        let dua = format!("{induk}-{}", varian(None, Some("2 kg")).unwrap());
        let lima = format!("{induk}-{}", varian(None, Some("5 kg")).unwrap());

        assert_eq!(dua, "CB-AFC-2KG");
        assert_eq!(lima, "CB-AFC-5KG");
        assert!(dua.starts_with(&induk) && lima.starts_with(&induk));
    }

    #[test]
    fn koreksi_manual_dinormalkan_ke_bentuk_yang_sama_dengan_rakitan() {
        // Tiga ketikan untuk barang yang sama harus bermuara ke satu SKU,
        // kalau tidak koreksi manual justru melahirkan penyimpangan yang
        // dihindari dengan merakit otomatis.
        assert_eq!(normalkan("cbsb afc 2kg").as_deref(), Some("CBSB-AFC-2KG"));
        assert_eq!(normalkan("CBSB-AFC-2KG").as_deref(), Some("CBSB-AFC-2KG"));
        assert_eq!(
            normalkan("  cbsb / afc__2kg  ").as_deref(),
            Some("CBSB-AFC-2KG")
        );
    }

    #[test]
    fn koreksi_manual_tanpa_huruf_maupun_angka_ditolak() {
        // Tanpa ini SKU bisa jadi "-" atau string kosong, dan produk berhenti
        // bisa dikenali sama sekali.
        assert_eq!(normalkan("   "), None);
        assert_eq!(normalkan("---"), None);
        assert_eq!(normalkan(""), None);
    }

    #[test]
    fn koreksi_manual_selalu_muat_di_kolom_sku() {
        let panjang = "a".repeat(500);
        let hasil = normalkan(&panjang).unwrap();
        assert!(hasil.chars().count() <= BASIS_MAKS);
        // Masih tersisa ruang untuk akhiran pembeda.
        assert!(dengan_akhiran(&hasil, 999).chars().count() <= SKU_MAKS);
    }

    #[test]
    fn akhiran_pembeda_memakai_pemisah_yang_sama() {
        assert_eq!(dengan_akhiran("CBSB-AFC", 2), "CBSB-AFC-2");
    }
}
