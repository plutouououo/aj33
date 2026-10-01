//! Pembatas login per username (bukan IP, semua request datang dari Astro di 127.0.0.1), di memori karena proses tunggal; jatah longgar agar bukan penguncian.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAKS_GAGAL: u32 = 10;
const JENDELA: Duration = Duration::from_secs(15 * 60);

#[derive(Debug)]
struct Catatan {
    gagal: u32,
    /// Percobaan gagal pertama pada jendela yang sedang berjalan.
    mulai: Instant,
}

#[derive(Debug)]
pub struct Throttle {
    maks: u32,
    jendela: Duration,
    // Mutex biasa (bukan async) karena bagian terkunci hanya operasi HashMap tanpa await sehingga tak menahan thread eksekutor.
    catatan: Mutex<HashMap<String, Catatan>>,
}

impl Throttle {
    pub fn baru() -> Self {
        Self::dengan(MAKS_GAGAL, JENDELA)
    }

    /// Dipakai test supaya tidak perlu menunggu 15 menit sungguhan.
    fn dengan(maks: u32, jendela: Duration) -> Self {
        Self {
            maks,
            jendela,
            catatan: Mutex::new(HashMap::new()),
        }
    }

    /// Username disamakan bentuknya agar "Owner" dan " owner " tak mendapat jatah masing-masing.
    fn kunci(username: &str) -> String {
        username.trim().to_lowercase()
    }

    /// Sisa waktu tunggu bila jatah habis, `None` bila boleh mencoba.
    pub fn sisa_tunggu(&self, username: &str) -> Option<Duration> {
        let kunci = Self::kunci(username);
        let catatan = self.catatan.lock().ok()?;
        let c = catatan.get(&kunci)?;

        let usia = c.mulai.elapsed();
        if c.gagal >= self.maks && usia < self.jendela {
            Some(self.jendela - usia)
        } else {
            None
        }
    }

    pub fn catat_gagal(&self, username: &str) {
        let kunci = Self::kunci(username);
        let Ok(mut catatan) = self.catatan.lock() else {
            // Mutex teracuni berarti thread lain panic; login tetap harus jalan karena kehilangan pembatas lebih baik daripada menolak semua orang.
            return;
        };

        // Dibersihkan tiap menulis agar peta tak tumbuh tanpa batas saat ribuan username berbeda dicoba.
        catatan.retain(|_, c| c.mulai.elapsed() < self.jendela);

        let c = catatan.entry(kunci).or_insert_with(|| Catatan {
            gagal: 0,
            mulai: Instant::now(),
        });

        // Jendela yang lewat dimulai ulang, bukan diteruskan, agar kegagalan sesekali berbulan-bulan tak menutup akun yang tak diserang.
        if c.mulai.elapsed() >= self.jendela {
            c.gagal = 0;
            c.mulai = Instant::now();
        }

        c.gagal = c.gagal.saturating_add(1);
    }

    /// Dipanggil setelah login berhasil karena percobaan gagal sebelumnya tak relevan lagi.
    pub fn bersihkan(&self, username: &str) {
        let kunci = Self::kunci(username);
        if let Ok(mut catatan) = self.catatan.lock() {
            catatan.remove(&kunci);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menahan_setelah_jatah_habis() {
        let t = Throttle::dengan(3, Duration::from_secs(60));

        for _ in 0..2 {
            t.catat_gagal("owner");
            assert!(t.sisa_tunggu("owner").is_none(), "belum habis, masih boleh");
        }

        t.catat_gagal("owner");
        assert!(
            t.sisa_tunggu("owner").is_some(),
            "jatah habis, harus tertahan"
        );
    }

    #[test]
    fn username_disamakan_bentuknya() {
        let t = Throttle::dengan(2, Duration::from_secs(60));
        t.catat_gagal("owner");
        t.catat_gagal("  OWNER  ");
        assert!(
            t.sisa_tunggu("Owner").is_some(),
            "huruf besar dan spasi tidak boleh memberi jatah terpisah"
        );
    }

    #[test]
    fn login_berhasil_menghapus_hitungan() {
        let t = Throttle::dengan(2, Duration::from_secs(60));
        t.catat_gagal("kasir");
        t.catat_gagal("kasir");
        assert!(t.sisa_tunggu("kasir").is_some());

        t.bersihkan("kasir");
        assert!(t.sisa_tunggu("kasir").is_none());
    }

    #[test]
    fn jendela_lewat_membuka_lagi() {
        let t = Throttle::dengan(1, Duration::from_millis(30));
        t.catat_gagal("pengepak");
        assert!(t.sisa_tunggu("pengepak").is_some());

        std::thread::sleep(Duration::from_millis(50));
        assert!(
            t.sisa_tunggu("pengepak").is_none(),
            "setelah jendela lewat harus boleh mencoba lagi"
        );
    }

    #[test]
    fn catatan_kedaluwarsa_dibuang() {
        let t = Throttle::dengan(5, Duration::from_millis(30));
        t.catat_gagal("lama");
        std::thread::sleep(Duration::from_millis(50));

        // Menulis untuk username lain memicu pembersihan.
        t.catat_gagal("baru");
        let catatan = t.catatan.lock().unwrap();
        assert!(!catatan.contains_key("lama"), "catatan basi harus dibuang");
        assert!(catatan.contains_key("baru"));
    }

    #[test]
    fn tiap_username_punya_jatah_sendiri() {
        let t = Throttle::dengan(1, Duration::from_secs(60));
        t.catat_gagal("owner");
        assert!(t.sisa_tunggu("owner").is_some());
        assert!(
            t.sisa_tunggu("kasir").is_none(),
            "username lain tidak boleh ikut tertahan"
        );
    }
}
