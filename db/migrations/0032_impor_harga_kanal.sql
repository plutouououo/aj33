-- Impor massal ikut membawa harga grosir, harga per kanal, dan batas grosir produk (migrasi 0031); NULL berarti kolomnya kosong di berkas: produk baru memakai bawaannya, produk lama tidak diubah.
ALTER TABLE "import_rows" ADD COLUMN "price_wholesale" DECIMAL(14,2);
ALTER TABLE "import_rows" ADD COLUMN "price_shopee" DECIMAL(14,2);
ALTER TABLE "import_rows" ADD COLUMN "price_shopee_wholesale" DECIMAL(14,2);
ALTER TABLE "import_rows" ADD COLUMN "price_tiktok" DECIMAL(14,2);
ALTER TABLE "import_rows" ADD COLUMN "price_tiktok_wholesale" DECIMAL(14,2);
-- Teks apa adanya seperti `variant_size`, diurai jadi kg saat commit, agar isi yang tak terbaca bisa diberi WARN di review.
ALTER TABLE "import_rows" ADD COLUMN "wholesale_threshold" VARCHAR(50);
