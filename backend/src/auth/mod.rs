//! Autentikasi dan otorisasi.
//!
//! Bentuk token mengikuti proyek lama (`shared/middleware/auth.ts`): JWT
//! HS256 dengan claim `sub` (id user) dan `role`, berlaku 8 jam. Yang berubah
//! hanya tempat penyimpanannya di sisi frontend -- dulu di client, sekarang
//! di cookie httpOnly supaya bisa dibaca Astro saat merender di server.

mod repo;
mod routes;
mod service;

pub use routes::router;

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Peran pengguna. Nilainya dikunci oleh CHECK constraint `users_role_check`
/// di database, jadi daftar di sini harus sama persis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Kasir,
    Pengepak,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Kasir => "kasir",
            Self::Pengepak => "pengepak",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Self::Owner),
            "kasir" => Some(Self::Kasir),
            "pengepak" => Some(Self::Pengepak),
            _ => None,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Pengguna yang sedang login, hasil pembacaan token. Handler mendapatkannya
/// lewat extractor, jadi tidak ada handler yang bisa lupa memeriksa token:
/// yang tidak menuliskannya di signature tidak akan bisa menyentuhnya.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: Role,
}
