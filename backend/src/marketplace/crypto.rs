//! Enkripsi token marketplace sebelum disimpan.
//!
//! Token akses toko sama berharganya dengan password: siapa pun yang
//! memegangnya bisa membaca dan mengubah pesanan di toko itu. Karena itu
//! kolom `platforms.access_token_encrypted` tidak pernah berisi token
//! mentah, bahkan di database development.
//!
//! Format ciphertext-nya sama persis dengan `crypto.util.ts` di proyek lama
//! (`iv:tag:data`, masing-masing base64, AES-256-GCM), supaya baris yang
//! sudah ada di database lama tetap bisa dibaca setelah refactor.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("format ciphertext tidak dikenal")]
    FormatSalah,
    #[error("ciphertext tidak bisa didekripsi (kunci salah atau data rusak)")]
    GagalDekripsi,
}

/// GCM memakai nonce 96 bit dan tag autentikasi 128 bit.
const PANJANG_NONCE: usize = 12;
const PANJANG_TAG: usize = 16;

pub fn encrypt(key: &[u8; 32], plaintext: &str) -> String {
    let cipher = Aes256Gcm::new(key.into());

    let mut nonce_bytes = [0u8; PANJANG_NONCE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // `aes-gcm` menempelkan tag di akhir ciphertext, sementara format lama
    // menyimpannya sebagai bagian terpisah. Dipisah di sini supaya bentuk
    // yang tersimpan tetap sama.
    let gabungan = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext.as_bytes(),
                aad: &[],
            },
        )
        .expect("enkripsi AES-GCM tidak bisa gagal dengan kunci dan nonce yang valid");

    let (data, tag) = gabungan.split_at(gabungan.len() - PANJANG_TAG);

    format!(
        "{}:{}:{}",
        B64.encode(nonce_bytes),
        B64.encode(tag),
        B64.encode(data)
    )
}

pub fn decrypt(key: &[u8; 32], payload: &str) -> Result<String, CryptoError> {
    let mut bagian = payload.split(':');
    let (Some(iv_b64), Some(tag_b64), Some(data_b64), None) =
        (bagian.next(), bagian.next(), bagian.next(), bagian.next())
    else {
        return Err(CryptoError::FormatSalah);
    };

    let nonce_bytes = B64.decode(iv_b64).map_err(|_| CryptoError::FormatSalah)?;
    let tag = B64.decode(tag_b64).map_err(|_| CryptoError::FormatSalah)?;
    let data = B64.decode(data_b64).map_err(|_| CryptoError::FormatSalah)?;

    if nonce_bytes.len() != PANJANG_NONCE || tag.len() != PANJANG_TAG {
        return Err(CryptoError::FormatSalah);
    }

    let mut gabungan = data;
    gabungan.extend_from_slice(&tag);

    let cipher = Aes256Gcm::new(key.into());
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &gabungan,
                aad: &[],
            },
        )
        .map_err(|_| CryptoError::GagalDekripsi)?;

    String::from_utf8(plaintext).map_err(|_| CryptoError::GagalDekripsi)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KUNCI: [u8; 32] = [7u8; 32];

    #[test]
    fn hasil_enkripsi_bisa_didekripsi_kembali() {
        let asli = "token-akses-rahasia";
        let terenkripsi = encrypt(&KUNCI, asli);

        assert!(!terenkripsi.contains(asli));
        assert_eq!(decrypt(&KUNCI, &terenkripsi).unwrap(), asli);
    }

    #[test]
    fn bentuknya_tiga_bagian_dipisah_titik_dua() {
        // Kompatibilitas dengan baris yang ditulis proyek lama bergantung
        // pada bentuk ini.
        let terenkripsi = encrypt(&KUNCI, "apa pun");
        assert_eq!(terenkripsi.split(':').count(), 3);
    }

    #[test]
    fn kunci_berbeda_tidak_bisa_membuka() {
        let terenkripsi = encrypt(&KUNCI, "token");
        let kunci_lain = [9u8; 32];
        assert!(matches!(
            decrypt(&kunci_lain, &terenkripsi),
            Err(CryptoError::GagalDekripsi)
        ));
    }

    #[test]
    fn ciphertext_yang_diubah_ditolak() {
        // Inilah gunanya GCM: data yang diutak-atik tidak diam-diam
        // menghasilkan plaintext yang salah, tapi ditolak.
        let terenkripsi = encrypt(&KUNCI, "token");
        let mut bagian: Vec<&str> = terenkripsi.split(':').collect();
        let rusak = B64.encode(b"data-yang-sudah-diubah");
        bagian[2] = &rusak;

        assert!(decrypt(&KUNCI, &bagian.join(":")).is_err());
    }

    #[test]
    fn format_asing_ditolak_sebagai_format_salah() {
        assert!(matches!(
            decrypt(&KUNCI, "bukan-format-yang-benar"),
            Err(CryptoError::FormatSalah)
        ));
    }
}
