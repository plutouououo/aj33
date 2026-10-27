-- Batas grosir per produk dipecah per kanal (walk in, Shopee, Tokopedia/TikTok); kolom wholesale_threshold_kg dari migrasi 0031 kini menjadi batas kanal toko.

-- NULL berarti produk memakai batas bawaan pricing_settings.wholesale_threshold_kg untuk kanal itu.
ALTER TABLE "products" ADD COLUMN "wholesale_threshold_kg_shopee" NUMERIC(8,3);
ALTER TABLE "products" ADD COLUMN "wholesale_threshold_kg_tiktok" NUMERIC(8,3);

ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_kg_shopee_check"
    CHECK (wholesale_threshold_kg_shopee IS NULL OR wholesale_threshold_kg_shopee > 0);
ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_kg_tiktok_check"
    CHECK (wholesale_threshold_kg_tiktok IS NULL OR wholesale_threshold_kg_tiktok > 0);

-- Di 0031 satu batas produk berlaku untuk semua kanal; disalin agar produk yang sudah punya batas tetap berharga sama di Shopee dan Tokopedia.
UPDATE "products"
SET "wholesale_threshold_kg_shopee" = "wholesale_threshold_kg",
    "wholesale_threshold_kg_tiktok" = "wholesale_threshold_kg"
WHERE "wholesale_threshold_kg" IS NOT NULL;

COMMENT ON COLUMN "products"."wholesale_threshold_kg" IS 'Batas grosir kanal toko (walk in) produk ini dalam kg; NULL memakai pricing_settings.wholesale_threshold_kg.';
COMMENT ON COLUMN "products"."wholesale_threshold_kg_shopee" IS 'Batas grosir kanal Shopee produk ini dalam kg; NULL memakai pricing_settings.wholesale_threshold_kg.';
COMMENT ON COLUMN "products"."wholesale_threshold_kg_tiktok" IS 'Batas grosir kanal Tokopedia/TikTok produk ini dalam kg; NULL memakai pricing_settings.wholesale_threshold_kg.';

-- Teks apa adanya seperti `wholesale_threshold` (migrasi 0032), diurai jadi kg saat commit.
ALTER TABLE "import_rows" ADD COLUMN "wholesale_threshold_shopee" VARCHAR(50);
ALTER TABLE "import_rows" ADD COLUMN "wholesale_threshold_tiktok" VARCHAR(50);
