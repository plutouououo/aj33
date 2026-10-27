//! Edit massal produk lewat xlsx: ekspor, ubah di spreadsheet, unggah, pratinjau per baris, lalu terapkan.

mod ekspor;
mod rencana;
mod repo;
mod routes;
mod terapkan;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use routes::router;

/// Judul kolom berkas; ekspor menulisnya dan unggahan mencocokkannya, jadi satu sumber di sini.
mod judul {
    pub const ID: &str = "id";
    pub const SKU: &str = "sku";
    pub const NAMA: &str = "nama";
    pub const NAMA_SEO: &str = "nama seo";
    pub const KATEGORI: &str = "kategori";
    pub const JENIS: &str = "jenis";
    pub const GRADE: &str = "grade";
    pub const MEREK: &str = "merek";
    pub const UKURAN: &str = "ukuran (kg)";
    pub const HARGA: &str = "harga ecer walk in";
    pub const GROSIR: &str = "harga grosir walk in";
    pub const SHOPEE: &str = "harga ecer shopee";
    pub const GROSIR_SHOPEE: &str = "harga grosir shopee";
    pub const TIKTOK: &str = "harga ecer tokopedia";
    pub const GROSIR_TIKTOK: &str = "harga grosir tokopedia";
    pub const BATAS: &str = "batas grosir walk in (kg)";
    pub const BATAS_SHOPEE: &str = "batas grosir shopee (kg)";
    pub const BATAS_TIKTOK: &str = "batas grosir tokopedia (kg)";
    pub const BATAS_STOK: &str = "batas stok menipis";
    pub const STATUS: &str = "status";

    pub const ID_BATCH: &str = "id batch";
    pub const SKU_INDUK: &str = "sku induk";
    pub const NO_BATCH: &str = "no batch";
    pub const ISI_KIRIMAN: &str = "isi kiriman";
    pub const SISA: &str = "sisa stok";
    pub const KEDALUWARSA: &str = "kedaluwarsa";
    pub const RAK: &str = "rak";
    pub const HARGA_BELI: &str = "harga beli";
}

/// Kolom yang diubah Owner pada kelompok utama; `Option` kosong berarti kolom dikosongkan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Kolom {
    Nama(String),
    NamaSeo(Option<String>),
    Sku(String),
    Kategori(Option<Uuid>),
    Jenis(Option<String>),
    Grade(Option<String>),
    Merek(Option<String>),
    Ukuran(Option<Decimal>),
    Harga(Decimal),
    Grosir(Option<Decimal>),
    Shopee(Option<Decimal>),
    GrosirShopee(Option<Decimal>),
    Tiktok(Option<Decimal>),
    GrosirTiktok(Option<Decimal>),
    Batas(Option<Decimal>),
    BatasShopee(Option<Decimal>),
    BatasTiktok(Option<Decimal>),
    BatasStok(i32),
    Aktif(bool),
}

/// Kolom batch pada kelompok stok; `Sisa` berisi target akhir, selisihnya dicatat sebagai penyesuaian di ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KolomBatch {
    Sisa(i32),
    Kedaluwarsa(Option<NaiveDate>),
    Rak(Option<String>),
    HargaBeli(Option<Decimal>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Aksi {
    Buat(Vec<Kolom>),
    Ubah {
        id: Uuid,
        kolom: Vec<Kolom>,
    },
    UbahBatch {
        produk_id: Uuid,
        batch_id: Uuid,
        kolom: Vec<KolomBatch>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusBaris {
    Baru,
    Edit,
    Tetap,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perubahan {
    pub kolom: String,
    pub lama: String,
    pub baru: String,
}

/// Satu baris berkas beserta rencananya; `aksi` hanya ada pada baris `Baru` dan `Edit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baris {
    /// Nomor baris di spreadsheet (header = 1).
    pub no: i32,
    pub label: String,
    pub status: StatusBaris,
    pub perubahan: Vec<Perubahan>,
    pub galat: Vec<String>,
    pub aksi: Option<Aksi>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HasilBaris {
    pub no: i32,
    pub ok: bool,
    pub pesan: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kelompok {
    Utama,
    Stok,
}

impl Kelompok {
    fn as_str(self) -> &'static str {
        match self {
            Self::Utama => "utama",
            Self::Stok => "stok",
        }
    }
}
