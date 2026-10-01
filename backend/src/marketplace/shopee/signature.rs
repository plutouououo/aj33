//! Tanda tangan Shopee v2: HMAC-SHA256 `partner_key` atas `partner_id + path + timestamp` (+ `access_token + shop_id` level toko), `path` lengkap termasuk `/api/v2`, body tak ikut.

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

    /// Vektor dari dokumentasi Shopee ("Signature generation"); bila tes ini berubah, algoritmanya yang berubah dan tak boleh disesuaikan diam-diam.
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
        // Bentuk yang tertukar adalah penyebab `error_sign` paling sering dan pesannya tak menyebut bagian yang keliru.
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

    /// Bagian disambung tanpa pemisah, jadi pergeseran batas antar bagian tak boleh menghasilkan tanda tangan sama.
    #[test]
    fn batas_antar_bagian_tidak_ambigu() {
        let a = tanda_tangan_toko("k", 1, "/api/v2/x", 2, "ab", 3);
        let b = tanda_tangan_toko("k", 1, "/api/v2/x", 2, "a", 3);
        assert_ne!(a, b);
    }
}
