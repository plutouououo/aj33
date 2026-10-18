//! Akses tabel `jejak_ui` (migrasi 0026).

use crate::error::AppResult;
use sqlx::PgExecutor;
use uuid::Uuid;

/// Tiga deret sejajar sama panjang; `area` kosong disimpan NULL karena deret `text[]` tak bisa memuat Option tanpa jalan memutar.
pub async fn simpan(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    halaman: &[String],
    area: &[String],
    target: &[String],
) -> AppResult<()> {
    sqlx::query!(
        r#"
        INSERT INTO jejak_ui (user_id, halaman, area, target)
        SELECT $1, h, NULLIF(a, ''), t
        FROM UNNEST($2::text[], $3::text[], $4::text[]) AS j(h, a, t)
        "#,
        user_id,
        halaman,
        area,
        target
    )
    .execute(db)
    .await?;

    Ok(())
}
