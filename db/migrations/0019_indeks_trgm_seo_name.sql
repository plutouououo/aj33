-- `seo_name` ditambahkan di 0006 dan ikut masuk klausa pencarian
-- (name/seo_name/sku) di `list_products`, tapi tidak pernah dapat indeks
-- trigram seperti `name` dan `sku` di 0004. Akibatnya pencarian jatuh ke
-- Seq Scan lagi -- persis masalah yang 0004 sudah perbaiki, kembali lewat
-- pintu belakang.
CREATE INDEX "idx_products_seo_name_trgm"
    ON "products" USING gin (("seo_name"::text) gin_trgm_ops);
