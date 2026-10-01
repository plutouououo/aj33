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
