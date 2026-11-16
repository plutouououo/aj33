-- Biaya layanan logistik Tokopedia/TikTok dikenakan per transaksi sebagai nominal tetap, bukan persen dari omzet barang.
ALTER TABLE "pricing_settings"
    ADD COLUMN "tiktok_logistics_fee" NUMERIC(14,2) NOT NULL DEFAULT 3030,
    ADD CONSTRAINT "pricing_settings_tiktok_logistics_fee_check" CHECK ("tiktok_logistics_fee" >= 0),
    DROP COLUMN "tiktok_logistics_percent";

COMMENT ON COLUMN "pricing_settings"."tiktok_logistics_fee" IS 'Rupiah "Biaya layanan logistik" TikTok Shop per transaksi, disimpan di transactions.platform_logistics_fee; transactions.platform_logistics_fee_percent hanya terisi di transaksi lama.';
