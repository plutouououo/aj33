//! Impor produk massal: upload -> staging -> review -> submit -> approve ->
//! commit di background -> retry.
//!
//! ```text
//! draft --submit--> pending_review --approve--> committing --selesai(worker)--> committed
//!   \-- cancel (dari draft/pending_review/approved) --> cancelled
//! committed --retry (fail_count>0)--> committing
//! ```
//!
//! `approved` divalidasi sebagai langkah antara (submit lompat status
//! ditolak persis di situ), tapi TIDAK pernah tertulis sebagai status yang
//! bertahan -- `approve` langsung menuliskan `committing` karena worker
//! langsung mengambilnya begitu disetujui. Lihat `service::approve`.

pub mod parse;
mod reader;
mod repo;
mod routes;
mod service;
mod worker;

use crate::error::{AppError, AppResult};

pub use routes::router;
pub use worker::jalankan_worker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStatus {
    Draft,
    PendingReview,
    Approved,
    Committing,
    Committed,
    Cancelled,
}

impl ImportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Committing => "committing",
            Self::Committed => "committed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw {
            "draft" => Ok(Self::Draft),
            "pending_review" => Ok(Self::PendingReview),
            "approved" => Ok(Self::Approved),
            "committing" => Ok(Self::Committing),
            "committed" => Ok(Self::Committed),
            "cancelled" => Ok(Self::Cancelled),
            lain => Err(AppError::conflict(format!(
                "Status impor \"{lain}\" tidak dikenal."
            ))),
        }
    }

    /// Langkah MAJU yang sah dari status ini. Retry (`committed` ->
    /// `committing`) BUKAN "maju" linear -- lihat `boleh_diulang`, dipisah
    /// karena syaratnya (`fail_count > 0`) bukan urusan enum ini.
    fn lanjutan(self) -> Option<Self> {
        match self {
            Self::Draft => Some(Self::PendingReview),
            Self::PendingReview => Some(Self::Approved),
            Self::Approved => Some(Self::Committing),
            Self::Committing => Some(Self::Committed),
            Self::Committed | Self::Cancelled => None,
        }
    }

    /// Memeriksa perpindahan status yang diminta -- pola sama dengan
    /// `tickets::TicketStatus::pindah_ke`.
    pub fn pindah_ke(self, tujuan: Self) -> AppResult<()> {
        if self.lanjutan() == Some(tujuan) {
            return Ok(());
        }
        Err(AppError::conflict(format!(
            "Batch impor berstatus \"{}\" tidak bisa dipindahkan ke \"{}\".",
            self.as_str(),
            tujuan.as_str()
        )))
    }

    pub fn boleh_dibatalkan(self) -> bool {
        matches!(self, Self::Draft | Self::PendingReview | Self::Approved)
    }

    pub fn boleh_diulang(self) -> bool {
        self == Self::Committed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tidak_boleh_melompat() {
        assert!(ImportStatus::Draft
            .pindah_ke(ImportStatus::Committing)
            .is_err());
        assert!(ImportStatus::Draft
            .pindah_ke(ImportStatus::Committed)
            .is_err());
    }

    #[test]
    fn status_tidak_boleh_mundur_atau_diulang_di_luar_retry() {
        assert!(ImportStatus::Approved
            .pindah_ke(ImportStatus::Draft)
            .is_err());
        assert!(ImportStatus::Committing
            .pindah_ke(ImportStatus::Committing)
            .is_err());
        assert!(ImportStatus::PendingReview
            .pindah_ke(ImportStatus::PendingReview)
            .is_err());
    }

    #[test]
    fn alur_maju_normal_diterima() {
        assert!(ImportStatus::Draft
            .pindah_ke(ImportStatus::PendingReview)
            .is_ok());
        assert!(ImportStatus::PendingReview
            .pindah_ke(ImportStatus::Approved)
            .is_ok());
        assert!(ImportStatus::Approved
            .pindah_ke(ImportStatus::Committing)
            .is_ok());
        assert!(ImportStatus::Committing
            .pindah_ke(ImportStatus::Committed)
            .is_ok());
    }

    #[test]
    fn hanya_committed_boleh_diulang() {
        assert!(ImportStatus::Committed.boleh_diulang());
        for s in [
            ImportStatus::Draft,
            ImportStatus::PendingReview,
            ImportStatus::Approved,
            ImportStatus::Committing,
            ImportStatus::Cancelled,
        ] {
            assert!(!s.boleh_diulang(), "{s:?} tidak seharusnya boleh diulang");
        }
    }

    #[test]
    fn cancel_hanya_dari_draft_pending_atau_approved() {
        assert!(ImportStatus::Draft.boleh_dibatalkan());
        assert!(ImportStatus::PendingReview.boleh_dibatalkan());
        assert!(ImportStatus::Approved.boleh_dibatalkan());
        assert!(!ImportStatus::Committing.boleh_dibatalkan());
        assert!(!ImportStatus::Committed.boleh_dibatalkan());
        assert!(!ImportStatus::Cancelled.boleh_dibatalkan());
    }

    #[test]
    fn status_asing_ditolak() {
        assert!(ImportStatus::parse("entah").is_err());
    }
}
