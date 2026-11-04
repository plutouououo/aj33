-- Batas grosir berpindah dari kg ke jumlah pack: grosir bila qty LEBIH dari batas, tanpa menimbang ukuran pack.

ALTER TABLE "products" DROP CONSTRAINT "products_wholesale_threshold_kg_check";
ALTER TABLE "products" DROP CONSTRAINT "products_wholesale_threshold_kg_shopee_check";
ALTER TABLE "products" DROP CONSTRAINT "products_wholesale_threshold_kg_tiktok_check";

-- Qty bulat: qty x ukuran > kg setara qty > FLOOR(kg / ukuran), jadi perilaku kasir lama tak berubah; hasil konversi minimal 1.
ALTER TABLE "products" ALTER COLUMN "wholesale_threshold_kg" TYPE INTEGER
    USING GREATEST(1, FLOOR("wholesale_threshold_kg" / COALESCE(NULLIF("variant_size", 0), 1)))::INTEGER;
ALTER TABLE "products" ALTER COLUMN "wholesale_threshold_kg_shopee" TYPE INTEGER
    USING GREATEST(1, FLOOR("wholesale_threshold_kg_shopee" / COALESCE(NULLIF("variant_size", 0), 1)))::INTEGER;
ALTER TABLE "products" ALTER COLUMN "wholesale_threshold_kg_tiktok" TYPE INTEGER
    USING GREATEST(1, FLOOR("wholesale_threshold_kg_tiktok" / COALESCE(NULLIF("variant_size", 0), 1)))::INTEGER;

ALTER TABLE "products" RENAME COLUMN "wholesale_threshold_kg" TO "wholesale_threshold_qty";
ALTER TABLE "products" RENAME COLUMN "wholesale_threshold_kg_shopee" TO "wholesale_threshold_qty_shopee";
ALTER TABLE "products" RENAME COLUMN "wholesale_threshold_kg_tiktok" TO "wholesale_threshold_qty_tiktok";

-- Bawaan baru satu angka pack untuk semua ukuran, jadi produk berukuran bukan 1 kg yang batasnya kosong dipatok ke konversi bawaan kg lama agar harganya tak bergeser.
UPDATE "products" p
SET "wholesale_threshold_qty" = GREATEST(1, FLOOR(s."wholesale_threshold_kg" / p."variant_size"))::INTEGER
FROM "pricing_settings" s
WHERE p."variant_size" > 0 AND p."variant_size" <> 1 AND p."price_wholesale" IS NOT NULL AND p."wholesale_threshold_qty" IS NULL;

UPDATE "products" p
SET "wholesale_threshold_qty_shopee" = GREATEST(1, FLOOR(s."wholesale_threshold_kg" / p."variant_size"))::INTEGER
FROM "pricing_settings" s
WHERE p."variant_size" > 0 AND p."variant_size" <> 1 AND p."price_shopee_wholesale" IS NOT NULL AND p."wholesale_threshold_qty_shopee" IS NULL;

UPDATE "products" p
SET "wholesale_threshold_qty_tiktok" = GREATEST(1, FLOOR(s."wholesale_threshold_kg" / p."variant_size"))::INTEGER
FROM "pricing_settings" s
WHERE p."variant_size" > 0 AND p."variant_size" <> 1 AND p."price_tiktok_wholesale" IS NOT NULL AND p."wholesale_threshold_qty_tiktok" IS NULL;

ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_qty_check" CHECK (wholesale_threshold_qty IS NULL OR wholesale_threshold_qty > 0);
ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_qty_shopee_check" CHECK (wholesale_threshold_qty_shopee IS NULL OR wholesale_threshold_qty_shopee > 0);
ALTER TABLE "products" ADD CONSTRAINT "products_wholesale_threshold_qty_tiktok_check" CHECK (wholesale_threshold_qty_tiktok IS NULL OR wholesale_threshold_qty_tiktok > 0);

ALTER TABLE "pricing_settings" DROP CONSTRAINT "pricing_settings_threshold_check";
ALTER TABLE "pricing_settings" ALTER COLUMN "wholesale_threshold_kg" TYPE INTEGER USING GREATEST(1, CEIL("wholesale_threshold_kg"))::INTEGER;
ALTER TABLE "pricing_settings" RENAME COLUMN "wholesale_threshold_kg" TO "wholesale_threshold_qty";
ALTER TABLE "pricing_settings" ADD CONSTRAINT "pricing_settings_threshold_check" CHECK ("wholesale_threshold_qty" > 0);

COMMENT ON COLUMN "products"."wholesale_threshold_qty" IS 'Grosir kanal toko bila qty pack LEBIH dari angka ini; NULL memakai pricing_settings.wholesale_threshold_qty.';
COMMENT ON COLUMN "products"."wholesale_threshold_qty_shopee" IS 'Grosir kanal Shopee bila qty pack LEBIH dari angka ini; NULL memakai pricing_settings.wholesale_threshold_qty.';
COMMENT ON COLUMN "products"."wholesale_threshold_qty_tiktok" IS 'Grosir kanal Tokopedia/TikTok bila qty pack LEBIH dari angka ini; NULL memakai pricing_settings.wholesale_threshold_qty.';
COMMENT ON COLUMN "pricing_settings"."wholesale_threshold_qty" IS 'Batas grosir bawaan dalam jumlah pack untuk produk yang tidak mengatur batasnya sendiri.';
