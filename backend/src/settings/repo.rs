//! Akses tabel `pricing_settings` (satu baris, dibuat migrasi 0022).

use crate::error::AppResult;
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

pub async fn batas_grosir_qty(db: impl PgExecutor<'_>) -> AppResult<i32> {
    let batas = sqlx::query_scalar!(r#"SELECT wholesale_threshold_qty FROM pricing_settings"#)
        .fetch_one(db)
        .await?;

    Ok(batas)
}

pub async fn ubah_batas_grosir_qty(
    db: impl PgExecutor<'_>,
    batas: i32,
    updated_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE pricing_settings
        SET wholesale_threshold_qty = $1, updated_by = $2, updated_at = now()
        "#,
        batas,
        updated_by
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Pecahan komisi dan layanan Shopee (0,1725 = 17,25%) yang dipakai checkout bila permintaan tak menyebut persen sendiri.
pub struct BiayaShopee {
    pub komisi: Decimal,
    pub layanan: Decimal,
}

pub async fn biaya_shopee(db: impl PgExecutor<'_>) -> AppResult<BiayaShopee> {
    let baris = sqlx::query!(
        r#"SELECT shopee_commission_percent, shopee_service_percent FROM pricing_settings"#
    )
    .fetch_one(db)
    .await?;

    Ok(BiayaShopee {
        komisi: baris.shopee_commission_percent,
        layanan: baris.shopee_service_percent,
    })
}

pub async fn ubah_biaya_shopee(
    db: impl PgExecutor<'_>,
    biaya: &BiayaShopee,
    updated_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE pricing_settings
        SET shopee_commission_percent = $1, shopee_service_percent = $2,
            updated_by = $3, updated_at = now()
        "#,
        biaya.komisi,
        biaya.layanan,
        updated_by
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Pecahan potongan Tokopedia/TikTok Shop dari omzet barang: komisi platform, komisi dinamis, dan biaya layanan logistik.
pub struct BiayaTiktok {
    pub komisi: Decimal,
    pub komisi_dinamis: Decimal,
    pub logistik: Decimal,
}

pub async fn biaya_tiktok(db: impl PgExecutor<'_>) -> AppResult<BiayaTiktok> {
    let baris = sqlx::query!(
        r#"
        SELECT tiktok_commission_percent, tiktok_dynamic_commission_percent, tiktok_logistics_percent
        FROM pricing_settings
        "#
    )
    .fetch_one(db)
    .await?;

    Ok(BiayaTiktok {
        komisi: baris.tiktok_commission_percent,
        komisi_dinamis: baris.tiktok_dynamic_commission_percent,
        logistik: baris.tiktok_logistics_percent,
    })
}

pub async fn ubah_biaya_tiktok(
    db: impl PgExecutor<'_>,
    biaya: &BiayaTiktok,
    updated_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE pricing_settings
        SET tiktok_commission_percent = $1, tiktok_dynamic_commission_percent = $2,
            tiktok_logistics_percent = $3, updated_by = $4, updated_at = now()
        "#,
        biaya.komisi,
        biaya.komisi_dinamis,
        biaya.logistik,
        updated_by
    )
    .execute(db)
    .await?;

    Ok(())
}
