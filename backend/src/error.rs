//! Bentuk response error `{ "error": { "code", "message" } }` dirakit hanya di sini; handler cukup mengembalikan `Err(AppError::...)`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Daftar code error seluruh backend, sengaja tertutup agar frontend mencocokkan dengan yakin; harus sama dengan `ErrorCode` di `contracts/api.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ValidationError,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TooManyRequests,
    InternalError,
}

impl ErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::ValidationError => "VALIDATION_ERROR",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::TooManyRequests => "TOO_MANY_REQUESTS",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    /// Status HTTP default tiap code, supaya tidak berbeda antar modul.
    fn status(self) -> StatusCode {
        match self {
            Self::ValidationError => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Error "diniatkan": kondisinya diperkirakan dan pesannya aman dibaca pengguna, beda dari error tak terduga yang pesan aslinya disembunyikan dan masuk log.
#[derive(Debug)]
pub struct AppError {
    code: ErrorCode,
    message: String,
    /// Penyebab asli untuk error 5xx. Tidak pernah ikut terkirim ke client.
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AppError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    /// 400 -- input dari pengguna salah bentuk atau tidak masuk akal.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, message)
    }

    /// 401 -- belum login, token tidak valid, atau username/password salah.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message)
    }

    /// 403 -- sudah login tapi perannya tidak berhak.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    /// 404 -- data atau endpoint tidak ada.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// 409: bentrok dengan kondisi sekarang (mis. stok tak cukup); permintaan ditolak karena terlalu sering dipakai pembatas login.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::TooManyRequests, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    /// 500: kegagalan di sisi kita bukan dari database; pesan tetap aman dibaca dan penyebab dicatat pemanggil lewat `tracing::error!`.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| &**e as &dyn std::error::Error)
    }
}

/// Error database tak pernah sampai ke client apa adanya (query dan host bisa ikut terbawa); yang dikirim pesan generik, detail masuk log lewat `source`.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self {
            code: ErrorCode::InternalError,
            message: "Terjadi kesalahan pada server.".to_string(),
            source: Some(Box::new(err)),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.code.status();

        // Error 5xx berarti kesalahan di sisi kita dan wajib tercatat lengkap karena client hanya menerima pesan generik.
        if status.is_server_error() {
            match &self.source {
                Some(cause) => tracing::error!(error = %self, cause = %cause, "internal error"),
                None => tracing::error!(error = %self, "internal error"),
            }
        }

        let body = Json(json!({
            "error": {
                "code": self.code.as_str(),
                "message": self.message,
            }
        }));

        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setiap_code_punya_status_yang_sesuai_kontrak_lama() {
        assert_eq!(ErrorCode::ValidationError.status(), StatusCode::BAD_REQUEST);
        assert_eq!(ErrorCode::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ErrorCode::Forbidden.status(), StatusCode::FORBIDDEN);
        assert_eq!(ErrorCode::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(ErrorCode::Conflict.status(), StatusCode::CONFLICT);
        assert_eq!(
            ErrorCode::InternalError.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn error_database_tidak_membocorkan_pesan_aslinya() {
        let err: AppError = sqlx::Error::RowNotFound.into();
        assert_eq!(err.code, ErrorCode::InternalError);
        assert_eq!(err.message, "Terjadi kesalahan pada server.");
    }
}
