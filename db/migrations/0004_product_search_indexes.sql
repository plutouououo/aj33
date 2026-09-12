-- Indeks untuk daftar dan pencarian produk.
--
-- Diukur pada katalog 100.006 baris (lihat scripts/seed-perf.sh):
--   - `name ILIKE '%kata%'` memaksa seq scan penuh, 63 ms sekali jalan --
--     dan dibayar dua kali per permintaan, karena query count(*) untuk
--     paginasi mengulang filter yang sama.
--   - `ORDER BY name` tanpa indeks menyortir di disk (external merge,
--     5 MB) begitu OFFSET-nya dalam: 146 ms di halaman 500.
--
-- Keduanya tumbuh linear terhadap jumlah produk, jadi katalog yang
-- bertambah membuat halaman kasir makin lambat justru saat toko makin
-- ramai.

-- Trigram: satu-satunya cara indeks menjangkau pola '%kata%', yang tidak
-- punya awalan tetap sehingga B-tree tak bisa dipakai. Ekstensi ini bagian
-- dari contrib PostgreSQL, tersedia di image resmi maupun Supabase.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Dipasang pada ekspresi `(kolom)::text`, persis bentuk yang dihasilkan
-- planner saat membandingkan kolom VARCHAR dengan ILIKE. Indeks pada kolom
-- mentahnya tidak akan dikenali cocok.
CREATE INDEX "idx_products_name_trgm"
    ON "products" USING gin (("name"::text) gin_trgm_ops);

CREATE INDEX "idx_products_sku_trgm"
    ON "products" USING gin (("sku"::text) gin_trgm_ops);

-- Urutan baku daftar produk. Dengan ini paginasi berjalan di atas indeks
-- yang sudah terurut, bukan menyortir ulang seluruh tabel tiap permintaan.
CREATE INDEX "idx_products_name" ON "products"("name");
