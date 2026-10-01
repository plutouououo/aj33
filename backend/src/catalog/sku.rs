//! Perakitan dan validasi SKU `[JENIS][GRADE]-[MEREK]-[UKURAN]` (huruf besar, 6-12 karakter): kode dari kamus atau inisial bebas, galat bila lewat 12, beku setelah dirakit, tanpa akhiran pembeda.

use std::collections::HashMap;
use std::fmt;

/// Panjang SKU, termasuk pemisah.
pub const SKU_MIN: usize = 6;
pub const SKU_MAKS: usize = 12;

const PEMISAH: char = '-';

/// Bagian atribut yang punya kode sendiri di kamus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bagian {
    Jenis,
    Grade,
    Merek,
    Ukuran,
}

impl Bagian {
    /// Nilai kolom `sku_codes.kind`, dikunci CHECK constraint sehingga daftar di sini harus sama persis.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jenis => "jenis",
            Self::Grade => "grade",
            Self::Merek => "merek",
            Self::Ukuran => "ukuran",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "jenis" => Some(Self::Jenis),
            "grade" => Some(Self::Grade),
            "merek" => Some(Self::Merek),
            "ukuran" => Some(Self::Ukuran),
            _ => None,
        }
    }

    /// Sebutan yang dibaca pengguna di pesan galat.
    fn label(self) -> &'static str {
        match self {
            Self::Jenis => "jenis produk",
            Self::Grade => "grade",
            Self::Merek => "merek",
            Self::Ukuran => "ukuran",
        }
    }
}

/// Kunci pencarian kamus: huruf besar tanpa spasi dan tanda baca, sehingga `SP 08`, `sp08`, `Sp-08` menemukan entri yang sama.
pub fn kunci(nilai: &str) -> String {
    nilai
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// Kamus kode pendek, dibaca dari tabel `sku_codes`.
#[derive(Debug, Default, Clone)]
pub struct Kamus {
    peta: HashMap<(Bagian, String), String>,
}

impl Kamus {
    pub fn baru(entri: impl IntoIterator<Item = (Bagian, String, String)>) -> Self {
        Self {
            peta: entri
                .into_iter()
                .map(|(bagian, nilai, kode)| ((bagian, kunci(&nilai)), kode))
                .collect(),
        }
    }

    fn kode(&self, bagian: Bagian, nilai: &str) -> Option<&str> {
        self.peta.get(&(bagian, kunci(nilai))).map(String::as_str)
    }
}

/// Alasan SKU tak bisa dipakai, pesannya ditulis untuk pemilik toko, bukan log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalatSku {
    /// Perakitan: tidak ada satu pun atribut yang terisi.
    TanpaAtribut,
    /// Tak tersisa huruf atau angka; terpisah dari `TanpaAtribut` karena koreksi manual tak merakit dari atribut.
    Kosong,
    /// Lebih dari `SKU_MAKS`; `calon` adalah bagian berinisial bebas yang bisa dipendekkan lewat kamus.
    TerlaluPanjang {
        sku: String,
        calon: Vec<(Bagian, String)>,
    },
    TerlaluPendek {
        sku: String,
    },
    KarakterIlegal {
        sku: String,
        karakter: char,
    },
    /// Pemisah di ujung atau berulang: `-CBSB`, `CBSB-`, `CBSB--AFC`.
    PemisahSalah {
        sku: String,
    },
}

impl fmt::Display for GalatSku {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TanpaAtribut => f.write_str(
                "SKU tidak bisa dirakit: jenis produk, grade, merek, dan ukuran semuanya kosong.",
            ),
            Self::Kosong => f.write_str(
                "SKU tidak boleh kosong, dan harus berisi huruf atau angka \
                 -- mis. CBSB-AFC-2KG.",
            ),
            Self::TerlaluPanjang { sku, calon } => {
                write!(
                    f,
                    "SKU \"{}\" {} karakter, melebihi batas {SKU_MAKS}.",
                    sku,
                    sku.chars().count()
                )?;
                if calon.is_empty() {
                    f.write_str(" Persingkat atribut produknya.")
                } else {
                    let daftar: Vec<String> = calon
                        .iter()
                        .map(|(bagian, nilai)| format!("{} \"{}\"", bagian.label(), nilai))
                        .collect();
                    write!(
                        f,
                        " Daftarkan kode pendek untuk {} di kamus kode SKU.",
                        daftar.join(" atau ")
                    )
                }
            }
            Self::TerlaluPendek { sku } => write!(
                f,
                "SKU \"{}\" cuma {} karakter, minimal {SKU_MIN}. \
                 Lengkapi jenis produk, grade, merek, atau ukurannya.",
                sku,
                sku.chars().count()
            ),
            Self::KarakterIlegal { sku, karakter } => write!(
                f,
                "SKU \"{sku}\" memuat karakter \"{karakter}\" yang tidak boleh dipakai. \
                 Hanya huruf A-Z, angka 0-9, dan tanda \"-\" sebagai pemisah."
            ),
            Self::PemisahSalah { sku } => write!(
                f,
                "SKU \"{sku}\" salah bentuk: tanda \"-\" hanya boleh jadi pemisah antar bagian, \
                 tidak di ujung dan tidak berulang."
            ),
        }
    }
}

/// Kode satu bagian beserta asalnya; `dari_kamus` menentukan apakah masih bisa dipendekkan.
struct Kode {
    teks: String,
    dari_kamus: bool,
}

fn kode_bagian(kamus: &Kamus, bagian: Bagian, nilai: Option<&str>) -> Kode {
    let Some(nilai) = nilai.map(str::trim).filter(|n| !n.is_empty()) else {
        return Kode {
            teks: String::new(),
            dari_kamus: false,
        };
    };

    if let Some(kode) = kamus.kode(bagian, nilai) {
        return Kode {
            teks: kode.to_string(),
            dari_kamus: true,
        };
    }

    Kode {
        teks: kode_bebas(bagian, nilai),
        dari_kamus: false,
    }
}

/// Kode yang diturunkan dari teks atribut sendiri, dipakai selama bagian belum ada di kamus.
fn kode_bebas(bagian: Bagian, nilai: &str) -> String {
    match bagian {
        // Merek selalu tiga huruf: satu kata ambil tiga huruf pertama (afco → AFC), dua kata 2+1 (best chicken → BEC), tiga kata atau lebih satu huruf dari tiga kata pertama.
        Bagian::Merek => {
            let kata: Vec<&str> = nilai.split_whitespace().collect();
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
        // Ukuran dibawa utuh ("2 kg" → "2KG") karena inisial membuang satuan dan ukuran tanpa satuan tak bermakna.
        Bagian::Ukuran => kunci(nilai),
        // Jenis dan grade diambil inisialnya (CBSB, HJA), kecuali yang mengandung angka dibawa utuh ("SP 08" → "SP08"); bila terlalu panjang kamus yang memendekkan, bukan pemenggalan.
        Bagian::Jenis | Bagian::Grade => {
            if nilai.chars().any(|c| c.is_ascii_digit()) {
                return kunci(nilai);
            }
            nilai
                .split_whitespace()
                .filter_map(|kata| kata.chars().find(|c| c.is_alphanumeric()))
                .flat_map(|c| c.to_uppercase())
                .collect()
        }
    }
}

/// Merakit SKU dari atribut: jenis dan grade menyatu tanpa pemisah, bagian kosong dilewati agar tak ada pemisah menggantung.
pub fn rakit(
    kamus: &Kamus,
    jenis_produk: Option<&str>,
    grade: Option<&str>,
    merek: Option<&str>,
    ukuran: Option<&str>,
) -> Result<String, GalatSku> {
    let jenis = kode_bagian(kamus, Bagian::Jenis, jenis_produk);
    let grade_kode = kode_bagian(kamus, Bagian::Grade, grade);
    let merek_kode = kode_bagian(kamus, Bagian::Merek, merek);
    let ukuran_kode = kode_bagian(kamus, Bagian::Ukuran, ukuran);

    let mut segmen: Vec<&str> = Vec::new();

    let kepala = format!("{}{}", jenis.teks, grade_kode.teks);
    if !kepala.is_empty() {
        segmen.push(&kepala);
    }
    if !merek_kode.teks.is_empty() {
        segmen.push(&merek_kode.teks);
    }
    if !ukuran_kode.teks.is_empty() {
        segmen.push(&ukuran_kode.teks);
    }

    if segmen.is_empty() {
        return Err(GalatSku::TanpaAtribut);
    }

    let sku = segmen.join(&PEMISAH.to_string());

    // Galat panjang menyebut dulu bagian yang masih berinisial bebas karena hanya itu yang bisa diperbuat pemilik.
    if sku.chars().count() > SKU_MAKS {
        let mut calon: Vec<(Bagian, String)> = [
            (Bagian::Jenis, jenis_produk, &jenis),
            (Bagian::Grade, grade, &grade_kode),
            (Bagian::Merek, merek, &merek_kode),
            (Bagian::Ukuran, ukuran, &ukuran_kode),
        ]
        .into_iter()
        .filter(|(_, _, kode)| !kode.dari_kamus && kode.teks.chars().count() > 1)
        .filter_map(|(bagian, nilai, _)| {
            nilai
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(|n| (bagian, n.to_string()))
        })
        .collect();

        // Bagian berkode terpanjang disebut lebih dulu karena mendaftarkannya paling mungkin menyelesaikan masalah.
        calon.sort_by_key(|(bagian, nilai)| {
            std::cmp::Reverse(kode_bebas(*bagian, nilai).chars().count())
        });

        return Err(GalatSku::TerlaluPanjang { sku, calon });
    }

    periksa(&sku)?;
    Ok(sku)
}

/// Memeriksa bentuk dan panjang SKU, satu pemeriksaan untuk hasil rakitan dan koreksi manual agar tak ada yang lolos lewat satu jalan tapi ditolak di jalan lain.
pub fn periksa(sku: &str) -> Result<(), GalatSku> {
    if sku.is_empty() {
        return Err(GalatSku::Kosong);
    }

    if let Some(karakter) = sku
        .chars()
        .find(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || *c == PEMISAH))
    {
        return Err(GalatSku::KarakterIlegal {
            sku: sku.to_string(),
            karakter,
        });
    }

    if sku.starts_with(PEMISAH) || sku.ends_with(PEMISAH) || sku.contains("--") {
        return Err(GalatSku::PemisahSalah {
            sku: sku.to_string(),
        });
    }

    let panjang = sku.chars().count();
    if panjang < SKU_MIN {
        return Err(GalatSku::TerlaluPendek {
            sku: sku.to_string(),
        });
    }
    if panjang > SKU_MAKS {
        return Err(GalatSku::TerlaluPanjang {
            sku: sku.to_string(),
            calon: Vec::new(),
        });
    }

    Ok(())
}

/// Membersihkan SKU ketikan manusia ke bentuk hasil rakitan (huruf besar, rentetan non-huruf-angka jadi satu pemisah) agar "cbsb afc / 2kg" dan "CBSB-AFC-2KG" tak jadi dua SKU; hanya jalur koreksi.
pub fn normalkan(kode: &str) -> Result<String, GalatSku> {
    let mut hasil = String::new();
    // Pemisah ditulis hanya bila ada isi sesudahnya sehingga tak ada pemisah di ujung depan/belakang.
    let mut tertunda = false;

    for c in kode.chars() {
        if c.is_alphanumeric() {
            if tertunda && !hasil.is_empty() {
                hasil.push(PEMISAH);
            }
            tertunda = false;
            hasil.extend(c.to_uppercase());
        } else {
            tertunda = true;
        }
    }

    periksa(&hasil)?;
    Ok(hasil)
}

/// Panjang maksimum satu kode kamus, lebih ketat dari `SKU_MAKS` agar satu bagian tak menghabiskan jatah SKU.
pub const KODE_MAKS: usize = SKU_MAKS - 2;

/// Alasan kode kamus ditolak, terpisah dari `GalatSku` karena aturannya beda (`-` tak boleh di kode satu bagian, batas `KODE_MAKS`, bukan 12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalatKode {
    Kosong,
    KarakterIlegal { kode: String, karakter: char },
    TerlaluPanjang { kode: String },
}

impl fmt::Display for GalatKode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kosong => f.write_str("Kode pendek wajib diisi."),
            Self::KarakterIlegal { kode, karakter } => write!(
                f,
                "Kode \"{kode}\" memuat karakter \"{karakter}\". Kode hanya boleh huruf A-Z \
                 dan angka 0-9, tanpa tanda \"-\" -- pemisah antar bagian dipasang otomatis."
            ),
            Self::TerlaluPanjang { kode } => write!(
                f,
                "Kode \"{}\" {} karakter, maksimal {KODE_MAKS}. Satu bagian tidak boleh \
                 menghabiskan jatah SKU yang cuma {SKU_MAKS} karakter.",
                kode,
                kode.chars().count()
            ),
        }
    }
}

/// Kode kamus: huruf besar dan angka saja, karena pemisah di kode satu bagian melahirkan SKU dengan lebih dari tiga bagian.
pub fn periksa_kode_kamus(kode: &str) -> Result<(), GalatKode> {
    if kode.is_empty() {
        return Err(GalatKode::Kosong);
    }

    if let Some(karakter) = kode
        .chars()
        .find(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit()))
    {
        return Err(GalatKode::KarakterIlegal {
            kode: kode.to_string(),
            karakter,
        });
    }

    if kode.chars().count() > KODE_MAKS {
        return Err(GalatKode::TerlaluPanjang {
            kode: kode.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kamus yang dipakai toko, sama dengan yang dipasang migrasi 0013; bila beda, test lulus sementara produksi menolak produk yang di sini diterima.
    fn kamus_toko() -> Kamus {
        Kamus::baru([
            (Bagian::Merek, "AFCO".to_string(), "AFC".to_string()),
            (Bagian::Merek, "BEST CHICKEN".to_string(), "BEC".to_string()),
            (Bagian::Merek, "OK CHICK".to_string(), "OKC".to_string()),
            (Bagian::Grade, "SP 08".to_string(), "08".to_string()),
            (Bagian::Grade, "SP 09".to_string(), "09".to_string()),
            (Bagian::Grade, "SP 10".to_string(), "10".to_string()),
        ])
    }

    fn kosong() -> Kamus {
        Kamus::default()
    }

    // Contoh yang ditulis di permintaan.

    #[test]
    fn contoh_ceker_bersih_super_besar_afco_2kg() {
        assert_eq!(
            rakit(
                &kosong(),
                Some("Ceker Bersih"),
                Some("Super Besar"),
                Some("AFCO"),
                Some("2 kg")
            ),
            Ok("CBSB-AFC-2KG".to_string())
        );
    }

    #[test]
    fn contoh_hati_jantung_ampela_afco() {
        assert_eq!(
            rakit(
                &kosong(),
                Some("Hati Jantung Ampela"),
                None,
                Some("AFCO"),
                None
            ),
            Ok("HJA-AFC".to_string())
        );
    }

    #[test]
    fn contoh_dada_sp08_best_chicken_1kg_butuh_kamus() {
        // Inisial bebas menghasilkan DSP08-BEC-1KG (13 karakter, lewat batas 12); tanpa kamus perakitan harus berhenti, bukan memenggal grade jadi "SP0".
        let galat = rakit(
            &kosong(),
            Some("Dada"),
            Some("SP 08"),
            Some("Best Chicken"),
            Some("1 kg"),
        )
        .unwrap_err();

        match &galat {
            GalatSku::TerlaluPanjang { sku, calon } => {
                assert_eq!(sku, "DSP08-BEC-1KG");
                assert_eq!(sku.chars().count(), 13);
                // Grade disebut lebih dulu karena kodenya ("SP08") terpanjang.
                assert_eq!(calon.first(), Some(&(Bagian::Grade, "SP 08".to_string())));
            }
            lain => panic!("harusnya TerlaluPanjang, dapat {lain:?}"),
        }

        // Pesan menyebut bagian yang harus didaftarkan, bukan sekadar "SKU terlalu panjang".
        let pesan = galat.to_string();
        assert!(pesan.contains("grade \"SP 08\""), "{pesan}");

        // Dengan entri kamus, hasilnya muat.
        assert_eq!(
            rakit(
                &kamus_toko(),
                Some("Dada"),
                Some("SP 08"),
                Some("Best Chicken"),
                Some("1 kg")
            ),
            Ok("D08-BEC-1KG".to_string())
        );
    }

    /// Kode grade SP sengaja angka saja; test ini menjaganya karena dengan "S08" produk toko ada yang tak muat dan baru ketahuan saat gagal menyimpan.
    #[test]
    fn kode_grade_sp_cukup_pendek_untuk_seluruh_jenis_yang_dijual() {
        assert_eq!(
            rakit(
                &kamus_toko(),
                Some("Ayam Utuh"),
                Some("SP 08"),
                Some("AFCO"),
                Some("2 kg")
            ),
            Ok("AU08-AFC-2KG".to_string())
        );
    }

    // Bentuk dan panjang.

    #[test]
    fn setiap_hasil_rakitan_lolos_aturan_bentuknya_sendiri() {
        for sku in [
            rakit(
                &kosong(),
                Some("Ceker Bersih"),
                Some("Super Besar"),
                Some("AFCO"),
                Some("2 kg"),
            ),
            rakit(
                &kosong(),
                Some("Hati Jantung Ampela"),
                None,
                Some("AFCO"),
                None,
            ),
            rakit(
                &kamus_toko(),
                Some("Dada"),
                Some("SP 08"),
                Some("Best Chicken"),
                Some("1 kg"),
            ),
            rakit(
                &kamus_toko(),
                Some("Ayam Utuh"),
                Some("SP 08"),
                Some("AFCO"),
                Some("2 kg"),
            ),
        ] {
            let sku = sku.unwrap();
            assert!(periksa(&sku).is_ok(), "{sku} tidak lolos periksa()");
            let n = sku.chars().count();
            assert!((SKU_MIN..=SKU_MAKS).contains(&n), "{sku} panjangnya {n}");
        }
    }

    #[test]
    fn panjang_di_luar_6_sampai_12_ditolak() {
        assert_eq!(periksa("CBSB-AFC-2KG"), Ok(())); // 12, batas atas
        assert_eq!(periksa("HJA-AFC"), Ok(())); // 7
        assert_eq!(periksa("CB-AFC"), Ok(())); // 6, batas bawah

        assert_eq!(
            periksa("CB-AF"),
            Err(GalatSku::TerlaluPendek {
                sku: "CB-AF".to_string()
            })
        );
        assert!(matches!(
            periksa("CBSB-AFC-25KG"),
            Err(GalatSku::TerlaluPanjang { .. })
        ));
    }

    #[test]
    fn hanya_huruf_besar_angka_dan_pemisah_yang_boleh() {
        assert_eq!(
            periksa("cbsb-afc-2kg"),
            Err(GalatSku::KarakterIlegal {
                sku: "cbsb-afc-2kg".to_string(),
                karakter: 'c'
            })
        );
        assert!(matches!(
            periksa("CBSB_AFC_2KG"),
            Err(GalatSku::KarakterIlegal { karakter: '_', .. })
        ));
        assert!(matches!(
            periksa("CBSB AFC 2KG"),
            Err(GalatSku::KarakterIlegal { karakter: ' ', .. })
        ));
        assert!(matches!(
            periksa("CBSB/AFC"),
            Err(GalatSku::KarakterIlegal { karakter: '/', .. })
        ));
    }

    #[test]
    fn pemisah_hanya_boleh_memisah() {
        for salah in ["-CBSB-AFC", "CBSB-AFC-", "CBSB--AFC"] {
            assert_eq!(
                periksa(salah),
                Err(GalatSku::PemisahSalah {
                    sku: salah.to_string()
                }),
                "{salah}"
            );
        }
    }

    #[test]
    fn spasi_dan_tanda_baca_dibuang_dari_tiap_bagian() {
        // "2 kg" → "2KG", "SP 08" → "SP08": hanya pemisah yang hilang, isinya utuh.
        assert_eq!(kode_bebas(Bagian::Ukuran, "2 kg"), "2KG");
        assert_eq!(kode_bebas(Bagian::Ukuran, "1,5 KG"), "15KG");
        assert_eq!(kode_bebas(Bagian::Grade, "SP 08"), "SP08");
    }

    #[test]
    fn huruf_besar_semua_apa_pun_cara_mengetiknya() {
        let a = rakit(
            &kosong(),
            Some("ceker bersih"),
            Some("super besar"),
            Some("afco"),
            None,
        );
        let b = rakit(
            &kosong(),
            Some("  Ceker  Bersih"),
            Some("SUPER BESAR "),
            Some(" AFCO"),
            None,
        );
        assert_eq!(a, b);
        assert_eq!(a, Ok("CBSB-AFC".to_string()));
    }

    // Bagian kosong.

    #[test]
    fn bagian_kosong_dilewati_bukan_menyisakan_pemisah_ganda() {
        // Bukan "HJA--AFC-".
        assert_eq!(
            rakit(
                &kosong(),
                Some("Hati Jantung Ampela"),
                Some("   "),
                Some("afco"),
                Some("")
            ),
            Ok("HJA-AFC".to_string())
        );
    }

    #[test]
    fn tanpa_satu_pun_atribut_ditolak_sebagai_kosong() {
        assert_eq!(
            rakit(&kosong(), None, Some(""), Some("   "), None),
            Err(GalatSku::TanpaAtribut)
        );
    }

    #[test]
    fn atribut_terlalu_sedikit_ditolak_karena_terlalu_pendek() {
        // "Dada" sendirian menghasilkan "D" (jauh di bawah 6) dan ditolak dengan pesan melengkapi atribut.
        let galat = rakit(&kosong(), Some("Dada"), None, None, None).unwrap_err();
        assert_eq!(
            galat,
            GalatSku::TerlaluPendek {
                sku: "D".to_string()
            }
        );
        assert!(galat.to_string().contains("Lengkapi"), "{galat}");
    }

    // Kamus.

    #[test]
    fn kamus_menang_atas_inisial_bebas() {
        let kamus = Kamus::baru([
            (Bagian::Jenis, "Ceker Bersih".to_string(), "CK".to_string()),
            (Bagian::Merek, "AFCO".to_string(), "AF".to_string()),
        ]);
        assert_eq!(
            rakit(
                &kamus,
                Some("Ceker Bersih"),
                Some("Super Besar"),
                Some("AFCO"),
                Some("2 kg")
            ),
            Ok("CKSB-AF-2KG".to_string())
        );
    }

    #[test]
    fn kunci_kamus_mengabaikan_spasi_tanda_baca_dan_besar_kecil_huruf() {
        let kamus = Kamus::baru([(Bagian::Grade, "SP 08".to_string(), "S08".to_string())]);
        // Tiga cara mengetik grade yang sama harus menemukan entri yang sama.
        for ejaan in ["SP 08", "sp08", "Sp-08"] {
            assert_eq!(
                rakit(
                    &kamus,
                    Some("Dada"),
                    Some(ejaan),
                    Some("Best Chicken"),
                    Some("1 kg")
                ),
                Ok("DS08-BEC-1KG".to_string()),
                "{ejaan}"
            );
        }
    }

    #[test]
    fn bagian_yang_sudah_dari_kamus_tidak_disarankan_lagi() {
        // Grade sudah punya entri sehingga yang disarankan hanya jenis dan ukuran.
        let kamus = Kamus::baru([(Bagian::Grade, "SP 08".to_string(), "SP08".to_string())]);
        let galat = rakit(
            &kamus,
            Some("Dada Fillet"),
            Some("SP 08"),
            Some("Best Chicken"),
            Some("1 kg"),
        )
        .unwrap_err();

        match galat {
            GalatSku::TerlaluPanjang { calon, .. } => {
                assert!(
                    !calon.iter().any(|(bagian, _)| *bagian == Bagian::Grade),
                    "grade sudah dari kamus, tidak boleh disarankan: {calon:?}"
                );
            }
            lain => panic!("harusnya TerlaluPanjang, dapat {lain:?}"),
        }
    }

    #[test]
    fn kode_kamus_hanya_huruf_besar_dan_angka() {
        assert_eq!(periksa_kode_kamus("S08"), Ok(()));
        assert_eq!(periksa_kode_kamus("08"), Ok(()));
        assert!(matches!(
            periksa_kode_kamus("s08"),
            Err(GalatKode::KarakterIlegal { .. })
        ));
        // Pemisah di kode satu bagian melahirkan SKU berbagian lebih dari tiga.
        assert!(matches!(
            periksa_kode_kamus("S-08"),
            Err(GalatKode::KarakterIlegal { karakter: '-', .. })
        ));
        assert_eq!(periksa_kode_kamus(""), Err(GalatKode::Kosong));
        // Satu bagian tidak boleh menghabiskan jatah seluruh SKU.
        assert_eq!(periksa_kode_kamus(&"A".repeat(KODE_MAKS)), Ok(()));
        assert!(matches!(
            periksa_kode_kamus(&"A".repeat(KODE_MAKS + 1)),
            Err(GalatKode::TerlaluPanjang { .. })
        ));
    }

    #[test]
    fn pesan_galat_kode_kamus_tidak_meminjam_aturan_sku() {
        // Dipinjam dari `GalatSku`, pesannya membantah dirinya sendiri (menyebut "-" boleh dan batas 12 untuk kode 11 karakter yang lewat batas 10).
        let pemisah = periksa_kode_kamus("S-08").unwrap_err().to_string();
        assert!(pemisah.contains("tanpa tanda"), "{pemisah}");

        let panjang = periksa_kode_kamus(&"A".repeat(KODE_MAKS + 1))
            .unwrap_err()
            .to_string();
        assert!(
            panjang.contains(&format!("maksimal {KODE_MAKS}")),
            "{panjang}"
        );

        let kosong = periksa_kode_kamus("").unwrap_err().to_string();
        assert!(!kosong.contains("SKU"), "{kosong}");
    }

    // Inisial bebas per bagian.

    #[test]
    fn merek_selalu_tiga_huruf_dibagi_menurut_jumlah_katanya() {
        assert_eq!(kode_bebas(Bagian::Merek, "afco"), "AFC");
        assert_eq!(kode_bebas(Bagian::Merek, "best chicken"), "BEC");
        assert_eq!(kode_bebas(Bagian::Merek, "OK CHICK"), "OKC");
        assert_eq!(kode_bebas(Bagian::Merek, "bsb whole best chicken"), "BWB");
    }

    #[test]
    fn jenis_dan_grade_diambil_inisialnya_kecuali_yang_berangka() {
        assert_eq!(kode_bebas(Bagian::Jenis, "Ceker Bersih"), "CB");
        assert_eq!(kode_bebas(Bagian::Jenis, "Hati Jantung Ampela"), "HJA");
        assert_eq!(kode_bebas(Bagian::Grade, "Super Besar"), "SB");
        // Berangka dibawa utuh: "S0" berhenti menunjuk kelas ukuran yang mana.
        assert_eq!(kode_bebas(Bagian::Grade, "SP 08"), "SP08");
    }

    // Koreksi manual.

    #[test]
    fn koreksi_manual_dinormalkan_ke_bentuk_yang_sama_dengan_rakitan() {
        // Tiga ketikan untuk barang yang sama harus bermuara ke satu SKU, kalau tidak koreksi manual melahirkan penyimpangan.
        for ketikan in ["cbsb afc 2kg", "CBSB-AFC-2KG", "  cbsb / afc__2kg  "] {
            assert_eq!(
                normalkan(ketikan),
                Ok("CBSB-AFC-2KG".to_string()),
                "{ketikan}"
            );
        }
    }

    #[test]
    fn koreksi_manual_tunduk_pada_batas_panjang_yang_sama() {
        assert!(matches!(
            normalkan("cb af"),
            Err(GalatSku::TerlaluPendek { .. })
        ));
        assert!(matches!(
            normalkan("cbsb afc 2kg extra"),
            Err(GalatSku::TerlaluPanjang { .. })
        ));
    }

    #[test]
    fn koreksi_manual_tanpa_huruf_maupun_angka_ditolak() {
        // Tanpa ini SKU bisa jadi "-" atau kosong dan produk tak bisa dikenali.
        for ketikan in ["   ", "---", ""] {
            assert_eq!(normalkan(ketikan), Err(GalatSku::Kosong), "{ketikan}");
        }
        // Pesan tak boleh menyuruh mengisi atribut karena jalur koreksi tak merakit dari atribut.
        let pesan = normalkan("---").unwrap_err().to_string();
        assert!(!pesan.contains("jenis produk"), "{pesan}");
    }

    #[test]
    fn tanda_baca_di_koreksi_manual_jadi_pemisah_bukan_penolakan() {
        // Ketikan pemilik dibersihkan, bukan ditolak; yang ditolak hanya karakter tersisa yang tak boleh ada di SKU (mis. huruf beraksen).
        assert_eq!(normalkan("CBSB@AFC"), Ok("CBSB-AFC".to_string()));
        assert_eq!(
            normalkan("CBSB / AFC . 2KG"),
            Ok("CBSB-AFC-2KG".to_string())
        );
        assert!(matches!(
            normalkan("CBSB-AFÉ"),
            Err(GalatSku::KarakterIlegal {
                karakter: '\u{c9}',
                ..
            })
        ));
    }

    #[test]
    fn koreksi_manual_memotong_di_batas_karakter_bukan_byte() {
        // Panjang dihitung dalam karakter, bukan byte, agar SKU multi-byte tak diterima/ditolak berdasarkan angka salah.
        let galat = normalkan(&"é".repeat(7)).unwrap_err();
        assert!(
            matches!(galat, GalatSku::KarakterIlegal { .. }),
            "{galat:?}"
        );
    }

    // Awalan varian.

    #[test]
    fn varian_yang_beda_ukuran_berbagi_awalan_dengan_induknya() {
        // Ini yang membuat pencarian "CBSB-AFC" menemukan semua ukurannya; ukuran bagian terakhir sehingga awalannya otomatis sama.
        let dua = rakit(
            &kosong(),
            Some("Ceker Bersih"),
            Some("Super Besar"),
            Some("AFCO"),
            Some("2 kg"),
        )
        .unwrap();
        let lima = rakit(
            &kosong(),
            Some("Ceker Bersih"),
            Some("Super Besar"),
            Some("AFCO"),
            Some("5 kg"),
        )
        .unwrap();

        assert_eq!(dua, "CBSB-AFC-2KG");
        assert_eq!(lima, "CBSB-AFC-5KG");
        assert!(dua.starts_with("CBSB-AFC") && lima.starts_with("CBSB-AFC"));
    }

    #[test]
    fn atribut_sama_persis_menghasilkan_sku_sama_persis() {
        // Yang menolak duplikat adalah database (indeks unik, `repo::sku_dipakai`); modul ini hanya harus konsisten agar dua barang sama tak lolos sebagai dua SKU.
        let a = rakit(
            &kosong(),
            Some("Ceker Bersih"),
            Some("Super Besar"),
            Some("AFCO"),
            Some("2 kg"),
        );
        let b = rakit(
            &kosong(),
            Some("Ceker Bersih"),
            Some("Super Besar"),
            Some("AFCO"),
            Some("2 kg"),
        );
        assert_eq!(a, b);
    }
}
