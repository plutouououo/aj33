//! Tiket packing satu arah (`unassigned` → `assigned` → `packing` → `packed` → `handed_over`); stok baru berkurang saat `handed_over`, jadi status tak boleh melompat atau mundur.

mod repo;
mod routes;
mod service;

pub use routes::router;

use crate::error::{AppError, AppResult};

/// Status tiket. Nilainya dibatasi CHECK constraint `tickets_status_check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketStatus {
    Unassigned,
    Assigned,
    Packing,
    Packed,
    HandedOver,
}

impl TicketStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unassigned => "unassigned",
            Self::Assigned => "assigned",
            Self::Packing => "packing",
            Self::Packed => "packed",
            Self::HandedOver => "handed_over",
        }
    }

    /// Status dari database dibaca lewat sini; nilai asing berarti ditulis versi lain atau diubah manual dan lebih baik ditolak.
    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw {
            "unassigned" => Ok(Self::Unassigned),
            "assigned" => Ok(Self::Assigned),
            "packing" => Ok(Self::Packing),
            "packed" => Ok(Self::Packed),
            "handed_over" => Ok(Self::HandedOver),
            _ => Err(AppError::conflict("Status tiket tidak dikenal.")),
        }
    }

    /// Langkah berikutnya yang sah dari status ini.
    fn lanjutan(self) -> Option<Self> {
        match self {
            Self::Assigned => Some(Self::Packing),
            Self::Packing => Some(Self::Packed),
            Self::Packed => Some(Self::HandedOver),
            // `unassigned` hanya bergerak lewat penugasan dan `handed_over` adalah akhir.
            Self::Unassigned | Self::HandedOver => None,
        }
    }

    /// Memeriksa perpindahan status; menolak lompatan (`assigned` ke `handed_over` melewati cek "semua dikemas") dan pengulangan (`handed_over` ke `handed_over` mengurangi stok dua kali).
    pub fn pindah_ke(self, tujuan: Self) -> AppResult<()> {
        if self.lanjutan() == Some(tujuan) {
            return Ok(());
        }

        Err(AppError::conflict(format!(
            "Tiket berstatus \"{}\" tidak bisa dipindahkan ke \"{}\".",
            self.as_str(),
            tujuan.as_str()
        )))
    }

    /// Tiket selesai dikemas tak boleh berpindah tangan: yang memeriksa isinya harus orang yang menyerahkan.
    fn masih_bisa_ditugaskan(self) -> bool {
        matches!(self, Self::Unassigned | Self::Assigned | Self::Packing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alur_normal_dari_awal_sampai_serah_terima() {
        assert!(TicketStatus::Assigned
            .pindah_ke(TicketStatus::Packing)
            .is_ok());
        assert!(TicketStatus::Packing
            .pindah_ke(TicketStatus::Packed)
            .is_ok());
        assert!(TicketStatus::Packed
            .pindah_ke(TicketStatus::HandedOver)
            .is_ok());
    }

    #[test]
    fn status_tidak_boleh_melompat() {
        // Melompat ke serah-terima melewati pemeriksaan "semua barang sudah dikemas".
        assert!(TicketStatus::Assigned
            .pindah_ke(TicketStatus::HandedOver)
            .is_err());
        assert!(TicketStatus::Unassigned
            .pindah_ke(TicketStatus::Packing)
            .is_err());
    }

    #[test]
    fn status_tidak_boleh_mundur_atau_diulang() {
        // Pengulangan serah-terima paling mahal: stok berkurang dua kali untuk satu pesanan.
        assert!(TicketStatus::HandedOver
            .pindah_ke(TicketStatus::HandedOver)
            .is_err());
        assert!(TicketStatus::Packed
            .pindah_ke(TicketStatus::Packing)
            .is_err());
    }

    #[test]
    fn penugasan_berhenti_setelah_barang_selesai_dikemas() {
        assert!(TicketStatus::Unassigned.masih_bisa_ditugaskan());
        assert!(TicketStatus::Assigned.masih_bisa_ditugaskan());
        assert!(TicketStatus::Packing.masih_bisa_ditugaskan());
        assert!(!TicketStatus::Packed.masih_bisa_ditugaskan());
        assert!(!TicketStatus::HandedOver.masih_bisa_ditugaskan());
    }

    #[test]
    fn status_asing_di_database_ditolak() {
        assert!(TicketStatus::parse("packing").is_ok());
        assert!(TicketStatus::parse("entah").is_err());
    }
}
