//! Endpoint pelanggan.

use super::repo::{
    self, Customer, CustomerFilter, CustomerPatch, FavoriteProduct, PurchaseRow, Urutan,
};
use crate::auth::{CurrentUser, Role};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/customers", get(list_customers).post(create_customer))
        .route(
            "/customers/{id}",
            get(get_customer)
                .patch(update_customer)
                .delete(delete_customer),
        )
}

/// Batas atas supaya satu request tidak bisa menarik seluruh tabel.
const LIMIT_MAKS: i64 = 200;

/// Panjang kolom skema awal diperiksa di sini agar nama kepanjangan dijawab 400 berpesan jelas, bukan 500 dari Postgres.
const NAMA_MAKS: usize = 150;
const TELEPON_MAKS: usize = 30;
/// `address` TEXT tanpa batas di database; batas di sini soal yang masuk akal diketik sebagai alamat agar kolom tak jadi tempat menempel apa saja.
const ALAMAT_MAKS: usize = 500;

/// Riwayat dan produk favorit yang ikut pada halaman detail.
const RIWAYAT: i64 = 20;
const FAVORIT: i64 = 5;

#[derive(Debug, Deserialize)]
struct ListQuery {
    search: Option<String>,
    /// `belanja`, `walk_in`, atau `marketplace`.
    scope: Option<String>,
    /// `nama` (baku), `belanja`, atau `terbaru`.
    sort: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct PaginatedCustomers {
    data: Vec<Customer>,
    page: i64,
    limit: i64,
    total: i64,
}

/// Terbuka untuk semua peran login karena kasir membacanya saat checkout dan halaman pesanan juga.
async fn list_customers(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<PaginatedCustomers>> {
    let limit = q.limit.unwrap_or(50).clamp(1, LIMIT_MAKS);
    let page = q.page.unwrap_or(1).max(1);

    let scope = match q.scope.as_deref() {
        None | Some("") => None,
        Some(s @ ("belanja" | "walk_in" | "marketplace")) => Some(s.to_string()),
        Some(lain) => {
            return Err(AppError::bad_request(format!(
                "Saringan '{lain}' tidak dikenal."
            )))
        }
    };

    let filter = CustomerFilter {
        search: q
            .search
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        scope,
        limit,
        offset: (page - 1) * limit,
    };

    let (data, total) =
        repo::list_customers(&state.pool, &filter, &Urutan::parse(q.sort.as_deref())).await?;

    Ok(Json(PaginatedCustomers {
        data,
        page,
        limit,
        total,
    }))
}

/// Pelanggan beserta yang hanya berguna saat membuka satu orang: riwayat belanja dan barang yang paling sering dibeli.
#[derive(Debug, Serialize)]
struct CustomerDetail {
    #[serde(flatten)]
    customer: Customer,
    favorite_products: Vec<FavoriteProduct>,
    purchases: Vec<PurchaseRow>,
}

async fn get_customer(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CustomerDetail>> {
    let customer = repo::find_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::not_found("Pelanggan tidak ditemukan."))?;

    Ok(Json(CustomerDetail {
        favorite_products: repo::favorite_products(&state.pool, id, FAVORIT).await?,
        purchases: repo::purchases(&state.pool, id, RIWAYAT).await?,
        customer,
    }))
}

// --- Pemeriksaan isian ---

fn periksa_nama(nama: &str) -> AppResult<()> {
    if nama.is_empty() {
        return Err(AppError::bad_request("Nama pelanggan wajib diisi."));
    }
    if nama.chars().count() > NAMA_MAKS {
        return Err(AppError::bad_request(format!(
            "Nama pelanggan terlalu panjang (maksimal {NAMA_MAKS} karakter)."
        )));
    }
    Ok(())
}

/// Membersihkan isian opsional: spasi dibuang dan yang jadi kosong dianggap "tidak diisi", bukan string kosong yang tampil sebagai baris alamat hampa.
fn bersihkan(nilai: Option<String>, maks: usize, nama: &str) -> AppResult<Option<String>> {
    let Some(nilai) = nilai
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };

    if nilai.chars().count() > maks {
        return Err(AppError::bad_request(format!(
            "{nama} terlalu panjang (maksimal {maks} karakter)."
        )));
    }

    Ok(Some(nilai))
}

// --- Tambah, ubah, hapus ---

#[derive(Debug, Deserialize)]
struct CustomerCreateRequest {
    name: String,
    phone: Option<String>,
    address: Option<String>,
}

async fn create_customer(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CustomerCreateRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Customer>)> {
    user.require(&[Role::Kasir, Role::Owner])?;

    let name = body.name.trim();
    periksa_nama(name)?;

    let phone = bersihkan(body.phone, TELEPON_MAKS, "Nomor telepon")?;
    let address = bersihkan(body.address, ALAMAT_MAKS, "Alamat")?;

    if let Some(phone) = phone.as_deref() {
        // Pelanggan langganan yang kembali tak jadi baris baru; dijawab 200, bukan 201, karena tak ada yang dibuat.
        if let Some(lama) = repo::find_by_phone(&state.pool, phone).await? {
            return Ok((axum::http::StatusCode::OK, Json(lama)));
        }
    }

    let customer =
        repo::insert_customer(&state.pool, name, phone.as_deref(), address.as_deref()).await?;
    Ok((axum::http::StatusCode::CREATED, Json(customer)))
}

/// Bidang tak disebut tak diubah, yang disebut `null` dikosongkan: `Option<Option<T>>` (`None` = tak disebut, `Some(None)` = null).
#[derive(Debug, Deserialize)]
struct CustomerUpdateRequest {
    name: Option<String>,
    #[serde(default, deserialize_with = "sebut")]
    phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "sebut")]
    address: Option<Option<String>>,
}

/// Membedakan "tak disebut" dari "disebut null"; tanpa ini serde memetakan keduanya ke `None` dan mengosongkan telepon mustahil.
fn sebut<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

async fn update_customer(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CustomerUpdateRequest>,
) -> AppResult<Json<Customer>> {
    user.require(&[Role::Kasir, Role::Owner])?;

    let name = match body.name {
        Some(name) => {
            let name = name.trim().to_string();
            periksa_nama(&name)?;
            Some(name)
        }
        None => None,
    };

    let patch = CustomerPatch {
        name,
        phone: match body.phone {
            Some(v) => Some(bersihkan(v, TELEPON_MAKS, "Nomor telepon")?),
            None => None,
        },
        address: match body.address {
            Some(v) => Some(bersihkan(v, ALAMAT_MAKS, "Alamat")?),
            None => None,
        },
    };

    let customer = repo::update_customer(&state.pool, id, &patch)
        .await?
        .ok_or_else(|| AppError::not_found("Pelanggan tidak ditemukan."))?;

    Ok(Json(customer))
}

/// Owner saja dan hanya untuk pelanggan tanpa riwayat: yang pernah bertransaksi tak boleh hilang karena laporan dan riwayat merujuknya (foreign key sudah menolak, di sini hanya penjelasan).
async fn delete_customer(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    user.require(&[Role::Owner])?;

    let rujukan = repo::hitung_rujukan(&state.pool, id).await?;
    if rujukan.ada() {
        return Err(AppError::conflict(format!(
            "Pelanggan ini masih punya {} transaksi kasir (belum void) dan {} pesanan marketplace, \
             jadi tidak bisa dihapus tanpa mengubah laporan yang sudah terbit.",
            rujukan.transactions, rujukan.orders
        )));
    }

    if !repo::delete_customer(&state.pool, id).await? {
        return Err(AppError::not_found("Pelanggan tidak ditemukan."));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
