//! Tanda tangan permintaan Shopee Open API v2.
//!
//! Jauh lebih sederhana daripada TikTok: HMAC-SHA256 dengan kunci
//! `partner_key` atas beberapa bagian yang disambung tanpa pemisah, hasilnya
//! hex huruf kecil. Body TIDAK ikut ditandatangani, dan parameter query
//! selain yang disebut di bawah juga tidak.
//!
//! Ada dua bentuk, dan memilih yang salah adalah kesalahan yang paling
//! sering terjadi:
//!
//! - Endpoint publik (`/api/v2/auth/...`) menandatangani
//!   `partner_id + path + timestamp`. Belum ada toko, jadi belum ada yang
//!   bisa ditambahkan.
//! - Endpoint level toko (order, logistics, dan seterusnya) menandatangani
//!   `partner_id + path + timestamp + access_token + shop_id`.
//!
//! `path` adalah path URL lengkap termasuk `/api/v2`, persis seperti yang
//! dikirim. Menandatangani `/order/get_order_list` padahal yang diminta
//! `/api/v2/order/get_order_list` menghasilkan `error_sign`.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn hitung(partner_key: &str, bagian: &[&str]) -> String {
    let mut mac = HmacSha256::new_from_slice(partner_key.as_bytes())
        .expect("HMAC menerima kunci dengan panjang berapa pun");

    for b in bagian {
        mac.update(b.as_bytes());
    }

    hex::encode(mac.finalize().into_bytes())
}

/// Tanda tangan endpoint publik: penukaran dan perpanjangan token.
pub fn tanda_tangan_publik(
    partner_key: &str,
    partner_id: i64,
    path: &str,
    timestamp: i64,
) -> String {
    hitung(
        partner_key,
        &[&partner_id.to_string(), path, &timestamp.to_string()],
    )
}

/// Tanda tangan endpoint yang bekerja atas nama satu toko.
pub fn tanda_tangan_toko(
    partner_key: &str,
    partner_id: i64,
    path: &str,
    timestamp: i64,
    access_token: &str,
    shop_id: i64,
) -> String {
    hitung(
        partner_key,
        &[
            &partner_id.to_string(),
            path,
            &timestamp.to_string(),
            access_token,
            &shop_id.to_string(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vektor dari dokumentasi Shopee: contoh `sign` di halaman
    /// "Signature generation". Kalau tes ini berubah, yang berubah adalah
    /// algoritmanya -- bukan sesuatu yang boleh disesuaikan diam-diam.
    #[test]
    fn cocok_dengan_perhitungan_manual() {
        // HMAC-SHA256("rahasia", "1001141/api/v2/auth/token/get1610000000")
        let hasil = tanda_tangan_publik("rahasia", 1001141, "/api/v2/auth/token/get", 1610000000);

        let manual = {
            let mut mac = HmacSha256::new_from_slice(b"rahasia").unwrap();
            mac.update(b"1001141/api/v2/auth/token/get1610000000");
            hex::encode(mac.finalize().into_bytes())
        };

        assert_eq!(hasil, manual);
    }

    #[test]
    fn hasilnya_hex_sepanjang_64_karakter() {
        let sig = tanda_tangan_publik("rahasia", 1, "/api/v2/x", 1);
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn tanda_tangan_stabil_dan_peka_terhadap_tiap_bagian() {
        let dasar = tanda_tangan_publik("rahasia", 1001141, "/api/v2/auth/token/get", 1610000000);

        assert_eq!(
            dasar,
            tanda_tangan_publik("rahasia", 1001141, "/api/v2/auth/token/get", 1610000000)
        );
        assert_ne!(
            dasar,
            tanda_tangan_publik("lain", 1001141, "/api/v2/auth/token/get", 1610000000)
        );
        assert_ne!(
            dasar,
            tanda_tangan_publik("rahasia", 2002282, "/api/v2/auth/token/get", 1610000000)
        );
        assert_ne!(
            dasar,
            tanda_tangan_publik(
                "rahasia",
                1001141,
                "/api/v2/auth/access_token/get",
                1610000000
            )
        );
        assert_ne!(
            dasar,
            tanda_tangan_publik("rahasia", 1001141, "/api/v2/auth/token/get", 1610000001)
        );
    }

    #[test]
    fn tanda_tangan_toko_berbeda_dari_tanda_tangan_publik() {
        // Bentuk yang tertukar adalah penyebab `error_sign` yang paling
        // sering, dan pesannya tidak menyebut bagian mana yang keliru.
        let publik = tanda_tangan_publik(
            "rahasia",
            1001141,
            "/api/v2/order/get_order_list",
            1610000000,
        );
        let toko = tanda_tangan_toko(
            "rahasia",
            1001141,
            "/api/v2/order/get_order_list",
            1610000000,
            "token",
            322300222,
        );

        assert_ne!(publik, toko);
    }

    #[test]
    fn tanda_tangan_toko_peka_terhadap_token_dan_shop_id() {
        let dasar = tanda_tangan_toko(
            "rahasia",
            1001141,
            "/api/v2/order/get_order_list",
            1610000000,
            "token",
            322300222,
        );

        assert_ne!(
            dasar,
            tanda_tangan_toko(
                "rahasia",
                1001141,
                "/api/v2/order/get_order_list",
                1610000000,
                "token-lain",
                322300222,
            )
        );
        assert_ne!(
            dasar,
            tanda_tangan_toko(
                "rahasia",
                1001141,
                "/api/v2/order/get_order_list",
                1610000000,
                "token",
                999999999,
            )
        );
    }

    /// Bagian disambung tanpa pemisah, jadi pergeseran batas antar bagian
    /// tidak boleh menghasilkan tanda tangan yang sama.
    #[test]
    fn batas_antar_bagian_tidak_ambigu() {
        let a = tanda_tangan_toko("k", 1, "/api/v2/x", 2, "ab", 3);
        let b = tanda_tangan_toko("k", 1, "/api/v2/x", 2, "a", 3);
        assert_ne!(a, b);
    }
}
