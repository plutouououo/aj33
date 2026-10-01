-- Persen komisi dan layanan Shopee pindah dari isian kasir ke pengaturan; nilai awal sama dengan tarif yang sebelumnya tertulis di kode.
ALTER TABLE "pricing_settings"
    ADD COLUMN "shopee_commission_percent" NUMERIC(6,4) NOT NULL DEFAULT 0.1725,
    ADD COLUMN "shopee_service_percent" NUMERIC(6,4) NOT NULL DEFAULT 0;

ALTER TABLE "pricing_settings"
    ADD CONSTRAINT "pricing_settings_shopee_commission_check"
        CHECK ("shopee_commission_percent" >= 0 AND "shopee_commission_percent" <= 1),
    ADD CONSTRAINT "pricing_settings_shopee_service_check"
        CHECK ("shopee_service_percent" >= 0 AND "shopee_service_percent" <= 1);

COMMENT ON COLUMN "pricing_settings"."shopee_commission_percent" IS 'Pecahan (0,1725 = 17,25%) commission_fee Shopee yang dipakai checkout bila permintaan tak menyebut persen sendiri.';
COMMENT ON COLUMN "pricing_settings"."shopee_service_percent" IS 'Pecahan service_fee Shopee (program opsional) yang dipakai checkout bila permintaan tak menyebut persen sendiri.';
