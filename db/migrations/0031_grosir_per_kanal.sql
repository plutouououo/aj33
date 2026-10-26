-- Harga grosir per kanal (Shopee, Tokopedia/TikTok) di samping kanal toko dari migrasi 0022, dan batas grosir per produk; uraian di docs/status-proyek.md.

-- NULL berarti grosir kanal itu belum diatur dan kasir memakai harga ecer kanal itu, bukan grosir toko.
ALTER TABLE "products" ADD COLUMN "price_shopee_wholesale" DECIMAL(14,2);
ALTER TABLE "products" ADD COLUMN "price_tiktok_wholesale" DECIMAL(14,2);

ALTER TABLE "products" ADD CONSTRAINT "products_price_shopee_wholesale_check"
    CHECK (price_shopee_wholesale IS NULL OR price_shopee_wholesale >= 0);
ALTER TABLE "products" ADD CONSTRAINT "products_price_tiktok_wholesale_check"
    CHECK (price_tiktok_wholesale IS NULL OR price_tiktok_wholesale >= 0);

COMMENT ON COLUMN "products"."price_shopee_wholesale" IS 'Harga grosir Shopee per pack, dipakai bila berat baris melebihi batas grosir produk.';
COMMENT ON COLUMN "products"."price_tiktok_wholesale" IS 'Harga grosir Tokopedia/TikTok per pack, dipakai bila berat baris melebihi batas grosir produk.';

-- NULL berarti produk memakai batas bawaan pricing_settings.wholesale_threshold_kg, jadi produk lama tak berubah saat migrasi.
ALTER TABLE "products" ADD COLUMN "wholesale_threshold_kg" NUMERIC(8,3);

ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_kg_check"
    CHECK (wholesale_threshold_kg IS NULL OR wholesale_threshold_kg > 0);

COMMENT ON COLUMN "products"."wholesale_threshold_kg" IS 'Batas grosir produk ini dalam kg, berlaku di semua kanal; NULL memakai pricing_settings.wholesale_threshold_kg.';
COMMENT ON COLUMN "pricing_settings"."wholesale_threshold_kg" IS 'Batas grosir bawaan dalam kg untuk produk yang tidak mengatur batasnya sendiri.';
