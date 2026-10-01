//! Autentikasi: JWT HS256 (claim `sub` dan `role`, 8 jam) mengikuti proyek lama, hanya penyimpanannya yang pindah ke cookie httpOnly agar Astro bisa membacanya.

mod repo;
mod routes;
mod service;
mod throttle;

pub use routes::router;
pub use throttle::Throttle;

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Peran pengguna dikunci CHECK `users_role_check` di database, jadi daftar di sini harus sama persis.
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

/// Pengguna yang login dari token, didapat lewat extractor sehingga handler yang tak menuliskannya di signature tak bisa menyentuhnya.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser {
    pub id: Uuid,
    pub role: Role,
}
