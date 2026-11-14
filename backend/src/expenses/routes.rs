//! Endpoint beban toko, khusus owner karena isinya pembukuan.

use super::repo::{self, BebanBaru, Expense, SaringanBeban};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::reports::{hari_ini, rentang_kueri};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/expenses", get(list_expenses).post(create_expense))
        .route(
            "/expenses/{id}",
            get(get_expense)
                .patch(update_expense)
                .delete(delete_expense),
        )
}

/// Kategori tetap agar laba rugi terkelompok rapi; belanja stok sengaja tak ada karena sudah masuk harga pokok lewat harga beli batch, begitu pula potongan marketplace dan PPh yang tercatat per transaksi.
const KATEGORI: [&str; 9] = [
    "Gaji & Upah",
    "Sewa Tempat",
    "Listrik & Air",
    "Kemasan & Es",
    "Transport & Bensin",
    "Iklan & Promosi",
    "Pulsa & Internet",
    "Peralatan & Perawatan",
    LAINNYA,
];

/// Sama dengan nama yang dipakai laporan untuk kategori kosong, jadi baris lama dan baru terkelompok bersama.
const LAINNYA: &str = "Lainnya";

const LIMIT_MAKS: i64 = 200;
const KETERANGAN_MAKS: usize = 500;

#[derive(Debug, Deserialize)]
struct ListQuery {
    period: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ExpenseList {
    data: Vec<Expense>,
    page: i64,
    limit: i64,
    total: i64,
    /// Jumlah rupiah seluruh baris yang cocok, bukan hanya halaman ini.
    total_amount: Decimal,
    /// Pilihan kategori untuk form, satu sumber dengan pemeriksaan di sini.
    categories: &'static [&'static str],
}

fn kategori_sah(nilai: &str) -> AppResult<String> {
    let nilai = nilai.trim();
    if KATEGORI.contains(&nilai) {
        Ok(nilai.to_string())
    } else {
        Err(AppError::bad_request(format!(
            "Kategori '{nilai}' tidak dikenal."
        )))
    }
}

async fn list_expenses(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<ExpenseList>> {
    user.require(&[Role::Owner])?;

    let (dari, sampai) = rentang_kueri(q.period.as_deref(), q.from, q.to)?.jendela();
    let kategori = match q.category.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(k) => Some(kategori_sah(k)?),
    };
    let limit = q.limit.unwrap_or(25).clamp(1, LIMIT_MAKS);
    let page = q.page.unwrap_or(1).max(1);

    let (data, total, total_amount) = repo::list(
        &state.pool,
        &SaringanBeban {
            dari,
            sampai,
            kategori,
            limit,
            offset: (page - 1) * limit,
        },
    )
    .await?;

    Ok(Json(ExpenseList {
        data,
        page,
        limit,
        total,
        total_amount,
        categories: &KATEGORI,
    }))
}

#[derive(Debug, Deserialize)]
struct ExpenseRequest {
    expense_date: NaiveDate,
    category: String,
    amount: Decimal,
    description: Option<String>,
}

/// Pemeriksaan isian terpisah dari handler (dengan `hari_ini` sebagai argumen) agar bisa diuji tanpa database.
fn periksa(body: ExpenseRequest, hari_ini: NaiveDate) -> AppResult<BebanBaru> {
    let category = kategori_sah(&body.category)?;

    // Tahun salah ketik (2062 untuk 2026) membuat beban hilang dari laporan bulan berjalan tanpa terasa.
    if body.expense_date > hari_ini {
        return Err(AppError::bad_request(
            "Tanggal beban tidak boleh setelah hari ini.",
        ));
    }

    if body.amount <= Decimal::ZERO {
        return Err(AppError::bad_request("Jumlah beban harus lebih dari nol."));
    }
    // Batas kolom DECIMAL(14,2): dua belas angka sebelum koma.
    if body.amount >= Decimal::from(1_000_000_000_000_i64) {
        return Err(AppError::bad_request("Jumlah beban terlalu besar."));
    }
    if body.amount.normalize().scale() > 2 {
        return Err(AppError::bad_request(
            "Jumlah beban paling banyak dua angka di belakang koma.",
        ));
    }

    let description = body
        .description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    if let Some(d) = &description {
        if d.chars().count() > KETERANGAN_MAKS {
            return Err(AppError::bad_request(format!(
                "Keterangan terlalu panjang (maksimal {KETERANGAN_MAKS} karakter)."
            )));
        }
    }
    // Tanpa keterangan, "Lainnya" di laporan tak bisa ditelusuri lagi bulan depan.
    if category == LAINNYA && description.is_none() {
        return Err(AppError::bad_request(
            "Keterangan wajib diisi untuk kategori Lainnya.",
        ));
    }

    Ok(BebanBaru {
        expense_date: body.expense_date,
        category,
        amount: body.amount,
        description,
    })
}

async fn get_expense(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Expense>> {
    user.require(&[Role::Owner])?;
    let beban = repo::find(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Beban tidak ditemukan."))?;
    Ok(Json(beban))
}

async fn create_expense(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ExpenseRequest>,
) -> AppResult<(StatusCode, Json<Expense>)> {
    user.require(&[Role::Owner])?;
    let beban = periksa(body, hari_ini())?;
    let simpan = repo::insert(&state.pool, &beban, user.id).await?;
    Ok((StatusCode::CREATED, Json(simpan)))
}

/// Mengganti seluruh isian (bukan sebagian) karena form selalu mengirim semuanya.
async fn update_expense(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ExpenseRequest>,
) -> AppResult<Json<Expense>> {
    user.require(&[Role::Owner])?;
    let beban = periksa(body, hari_ini())?;
    let simpan = repo::update(&state.pool, id, &beban)
        .await?
        .ok_or_else(|| AppError::not_found("Beban tidak ditemukan."))?;
    Ok(Json(simpan))
}

async fn delete_expense(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    user.require(&[Role::Owner])?;
    if !repo::delete(&state.pool, id).await? {
        return Err(AppError::not_found("Beban tidak ditemukan."));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tgl(t: i32, b: u32, h: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(t, b, h).unwrap()
    }

    fn isian(kategori: &str, jumlah: &str, keterangan: Option<&str>) -> ExpenseRequest {
        ExpenseRequest {
            expense_date: tgl(2026, 11, 10),
            category: kategori.to_string(),
            amount: jumlah.parse().unwrap(),
            description: keterangan.map(str::to_string),
        }
    }

    fn hari_ini_uji() -> NaiveDate {
        tgl(2026, 11, 13)
    }

    #[test]
    fn isian_sah_dirapikan() {
        let b = periksa(
            isian("  Listrik & Air ", "350000", Some("  token PLN  ")),
            hari_ini_uji(),
        )
        .unwrap();
        assert_eq!(b.category, "Listrik & Air");
        assert_eq!(b.description.as_deref(), Some("token PLN"));
        assert_eq!(b.amount, Decimal::from(350_000));
    }

    #[test]
    fn keterangan_kosong_menjadi_none() {
        let b = periksa(isian("Kemasan & Es", "20000", Some("   ")), hari_ini_uji()).unwrap();
        assert!(b.description.is_none());
    }

    #[test]
    fn kategori_tak_dikenal_ditolak() {
        assert!(periksa(isian("Belanja Ayam", "100000", None), hari_ini_uji()).is_err());
    }

    #[test]
    fn jumlah_nol_negatif_atau_tiga_desimal_ditolak() {
        assert!(periksa(isian("Sewa Tempat", "0", None), hari_ini_uji()).is_err());
        assert!(periksa(isian("Sewa Tempat", "-5000", None), hari_ini_uji()).is_err());
        assert!(periksa(isian("Sewa Tempat", "1000.555", None), hari_ini_uji()).is_err());
        assert!(periksa(isian("Sewa Tempat", "1000.50", None), hari_ini_uji()).is_ok());
    }

    #[test]
    fn tanggal_setelah_hari_ini_ditolak() {
        let mut body = isian("Sewa Tempat", "1000000", None);
        body.expense_date = tgl(2026, 11, 14);
        assert!(periksa(body, hari_ini_uji()).is_err());

        let mut body = isian("Sewa Tempat", "1000000", None);
        body.expense_date = hari_ini_uji();
        assert!(periksa(body, hari_ini_uji()).is_ok());
    }

    #[test]
    fn lainnya_wajib_berketerangan() {
        assert!(periksa(isian("Lainnya", "15000", None), hari_ini_uji()).is_err());
        assert!(periksa(isian("Lainnya", "15000", Some("parkir")), hari_ini_uji()).is_ok());
    }
}
