//! Tanda tangan permintaan TikTok Shop Open API.
//!
//! Algoritmanya: HMAC-SHA256 dengan kunci `app_secret`, atas rangkaian
//! `app_secret + path + sorted_params + body + app_secret`, hasilnya hex
//! huruf kecil.
//!
//! PERHATIAN: proyek lama menandai bagian ini "cocokkan ke dokumen App-mu
//! di Partner Center" dan memakai huruf BESAR. Dokumen resmi TikTok berada
//! di balik login, dan detailnya berbeda antar versi API. Sebelum dipakai ke
//! toko sungguhan, cocokkan urutan dan huruf besar/kecilnya dengan dokumen
//! App yang dipakai -- tanda tangan yang salah ditolak dengan kode error
//! yang tidak menjelaskan bagian mana yang keliru.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Menyusun bagian parameter untuk ditandatangani.
///
/// `sign` dan `access_token` sengaja dikecualikan: `sign` adalah hasil yang
/// sedang dihitung, dan `access_token` memang tidak ikut ditandatangani.
/// Sisanya diurutkan menurut nama, lalu digabung sebagai `namanilai` tanpa
/// pemisah.
pub fn susun_parameter(params: &[(String, String)]) -> String {
    let mut urut: Vec<&(String, String)> = params
        .iter()
        .filter(|(k, _)| k != "sign" && k != "access_token")
        .collect();

    urut.sort_by(|a, b| a.0.cmp(&b.0));

    urut.iter()
        .map(|(k, v)| format!("{k}{v}"))
        .collect::<Vec<_>>()
        .join("")
}

pub fn tanda_tangan(app_secret: &str, path: &str, parameter: &str, body: Option<&str>) -> String {
    let pesan = format!(
        "{app_secret}{path}{parameter}{}{app_secret}",
        body.unwrap_or_default()
    );

    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes())
        .expect("HMAC menerima kunci dengan panjang berapa pun");
    mac.update(pesan.as_bytes());

    hex::encode(mac.finalize().into_bytes())
}

/// Memeriksa tanda tangan webhook yang masuk.
///
/// Perbandingannya memakai `Mac::verify_slice`, yang membandingkan dalam
/// waktu tetap. Membandingkan dua string biasa dengan `==` akan berhenti di
/// byte pertama yang berbeda, dan selisih waktunya cukup untuk menebak
/// tanda tangan yang benar byte demi byte.
pub fn webhook_sah(app_key: &str, app_secret: &str, raw_body: &str, signature_hex: &str) -> bool {
    let Ok(diterima) = hex::decode(signature_hex) else {
        return false;
    };

    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes())
        .expect("HMAC menerima kunci dengan panjang berapa pun");
    mac.update(app_key.as_bytes());
    mac.update(raw_body.as_bytes());

    mac.verify_slice(&diterima).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_diurutkan_berdasarkan_nama() {
        let params = vec![
            ("timestamp".to_string(), "100".to_string()),
            ("app_key".to_string(), "kunci".to_string()),
            ("shop_cipher".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            susun_parameter(&params),
            "app_keykuncishop_cipherabctimestamp100"
        );
    }

    #[test]
    fn sign_dan_access_token_tidak_ikut_ditandatangani() {
        let params = vec![
            ("app_key".to_string(), "kunci".to_string()),
            ("sign".to_string(), "harus-diabaikan".to_string()),
            ("access_token".to_string(), "juga-diabaikan".to_string()),
        ];

        assert_eq!(susun_parameter(&params), "app_keykunci");
    }

    #[test]
    fn urutan_masukan_tidak_mengubah_hasil() {
        let a = vec![
            ("b".to_string(), "2".to_string()),
            ("a".to_string(), "1".to_string()),
        ];
        let b = vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ];

        assert_eq!(susun_parameter(&a), susun_parameter(&b));
    }

    #[test]
    fn tanda_tangan_stabil_dan_peka_terhadap_perubahan() {
        let dasar = tanda_tangan("rahasia", "/api/orders/search", "app_keykunci", None);

        assert_eq!(
            dasar,
            tanda_tangan("rahasia", "/api/orders/search", "app_keykunci", None)
        );
        assert_ne!(
            dasar,
            tanda_tangan("rahasia-lain", "/api/orders/search", "app_keykunci", None)
        );
        assert_ne!(
            dasar,
            tanda_tangan("rahasia", "/api/orders/detail", "app_keykunci", None)
        );
        assert_ne!(
            dasar,
            tanda_tangan("rahasia", "/api/orders/search", "app_keylain", None)
        );
        assert_ne!(
            dasar,
            tanda_tangan("rahasia", "/api/orders/search", "app_keykunci", Some("{}"))
        );
    }

    #[test]
    fn hasilnya_hex_sepanjang_64_karakter() {
        let sig = tanda_tangan("rahasia", "/path", "", None);
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn webhook_dengan_tanda_tangan_benar_diterima() {
        let body = r#"{"type":1,"data":{"order_id":"123"}}"#;
        let sah = {
            let mut mac = HmacSha256::new_from_slice(b"rahasia").unwrap();
            mac.update(b"app-key");
            mac.update(body.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        };

        assert!(webhook_sah("app-key", "rahasia", body, &sah));
    }

    #[test]
    fn webhook_dengan_body_diubah_ditolak() {
        let body = r#"{"type":1,"data":{"order_id":"123"}}"#;
        let sah = {
            let mut mac = HmacSha256::new_from_slice(b"rahasia").unwrap();
            mac.update(b"app-key");
            mac.update(body.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        };

        // Inti dari verifikasi ini: payload yang diubah di tengah jalan --
        // misalnya jumlah barang dinaikkan -- tidak boleh lolos.
        let diubah = r#"{"type":1,"data":{"order_id":"999"}}"#;
        assert!(!webhook_sah("app-key", "rahasia", diubah, &sah));
    }

    #[test]
    fn tanda_tangan_bukan_hex_ditolak_tanpa_panic() {
        assert!(!webhook_sah(
            "app-key",
            "rahasia",
            "{}",
            "bukan-hex-sama-sekali"
        ));
    }
}
