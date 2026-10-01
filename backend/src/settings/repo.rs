//! Akses tabel `pricing_settings` (satu baris, dibuat migrasi 0022).

use crate::error::AppResult;
use rust_decimal::Decimal;
use sqlx::PgExecutor;
use uuid::Uuid;

pub async fn batas_grosir_kg(db: impl PgExecutor<'_>) -> AppResult<Decimal> {
    let batas = sqlx::query_scalar!(r#"SELECT wholesale_threshold_kg FROM pricing_settings"#)
        .fetch_one(db)
        .await?;

    Ok(batas)
}

pub async fn ubah_batas_grosir_kg(
    db: impl PgExecutor<'_>,
    batas: Decimal,
    updated_by: Uuid,
) -> AppResult<()> {
    sqlx::query!(
        r#"
        UPDATE pricing_settings
        SET wholesale_threshold_kg = $1, updated_by = $2, updated_at = now()
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
