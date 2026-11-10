-- Potongan Tokopedia/TikTok Shop: komisi platform, komisi dinamis, dan biaya layanan logistik berupa persen; PPh dan biaya proses sama dengan Shopee.
ALTER TABLE "pricing_settings"
    ADD COLUMN "tiktok_commission_percent" NUMERIC(6,4) NOT NULL DEFAULT 0.0775,
    ADD COLUMN "tiktok_dynamic_commission_percent" NUMERIC(6,4) NOT NULL DEFAULT 0.065,
    ADD COLUMN "tiktok_logistics_percent" NUMERIC(6,4) NOT NULL DEFAULT 0.0505;

ALTER TABLE "pricing_settings"
    ADD CONSTRAINT "pricing_settings_tiktok_commission_check"
        CHECK ("tiktok_commission_percent" >= 0 AND "tiktok_commission_percent" <= 1),
    ADD CONSTRAINT "pricing_settings_tiktok_dynamic_commission_check"
        CHECK ("tiktok_dynamic_commission_percent" >= 0 AND "tiktok_dynamic_commission_percent" <= 1),
    ADD CONSTRAINT "pricing_settings_tiktok_logistics_check"
        CHECK ("tiktok_logistics_percent" >= 0 AND "tiktok_logistics_percent" <= 1);

COMMENT ON COLUMN "pricing_settings"."tiktok_commission_percent" IS 'Pecahan "Biaya komisi platform" TikTok Shop (0,0775 = 7,75%) dari omzet barang; disimpan di transactions.platform_commission_fee_percent.';
COMMENT ON COLUMN "pricing_settings"."tiktok_dynamic_commission_percent" IS 'Pecahan "Komisi dinamis" TikTok Shop dari omzet barang.';
COMMENT ON COLUMN "pricing_settings"."tiktok_logistics_percent" IS 'Pecahan "Biaya layanan logistik" TikTok Shop dari omzet barang, dihitung dari satu contoh pesanan (Rp3.030 dari Rp60.000) sehingga perlu dicek ulang terhadap pesanan lain.';

ALTER TABLE "transactions"
    ADD COLUMN "platform_dynamic_commission_fee_percent" DECIMAL(6,4) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_dynamic_commission_fee" DECIMAL(14,2) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_logistics_fee_percent" DECIMAL(6,4) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_logistics_fee" DECIMAL(14,2) NOT NULL DEFAULT 0;

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_tiktok_check"
    CHECK (
        platform_dynamic_commission_fee_percent >= 0 AND platform_dynamic_commission_fee_percent <= 1
        AND platform_dynamic_commission_fee >= 0
        AND platform_logistics_fee_percent >= 0 AND platform_logistics_fee_percent <= 1
        AND platform_logistics_fee >= 0
    );

-- Komisi dinamis dan biaya logistik hanya TikTok, biaya layanan hanya Shopee; PPh juga terisi di toko dan biaya lain nol di toko.
ALTER TABLE "transactions" DROP CONSTRAINT "transactions_platform_fee_channel_check";

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_channel_check"
    CHECK (
        CASE sales_channel
            WHEN 'shopee' THEN
                platform_dynamic_commission_fee_percent = 0 AND platform_dynamic_commission_fee = 0
                AND platform_logistics_fee_percent = 0 AND platform_logistics_fee = 0
            WHEN 'tiktok' THEN
                platform_service_fee_percent = 0 AND platform_service_fee = 0
            ELSE
                platform_commission_fee_percent = 0 AND platform_commission_fee = 0
                AND platform_service_fee_percent = 0 AND platform_service_fee = 0
                AND platform_dynamic_commission_fee_percent = 0 AND platform_dynamic_commission_fee = 0
                AND platform_logistics_fee_percent = 0 AND platform_logistics_fee = 0
                AND platform_order_processing_fee = 0
                AND (platform_withholding_tax = 0 OR sales_channel = 'toko')
        END
    );

-- Jumlah seluruh potongan platform, turunan database supaya laporan tak mengulang penjumlahan enam kolom.
ALTER TABLE "transactions"
    ADD COLUMN "platform_fee_total" DECIMAL(14,2)
        GENERATED ALWAYS AS (
            "platform_commission_fee" + "platform_service_fee"
            + "platform_dynamic_commission_fee" + "platform_logistics_fee"
            + "platform_withholding_tax" + "platform_order_processing_fee"
        ) STORED;
