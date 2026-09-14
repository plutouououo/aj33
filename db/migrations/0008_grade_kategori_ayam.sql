-- Warna diganti grade, kategori diisi, dan satuan ditinggalkan.
--
-- GRADE, BUKAN WARNA. Sumbu varian di toko ayam bukan warna -- tidak ada
-- ayam merah dan ayam biru. Yang membedakan dua baris katalog adalah mutu
-- dan kelas ukurannya: "SP 06", "SP 08", "Super Besar", "S Super", "B".
-- Kolomnya dinamai ulang, bukan sekadar labelnya di layar diganti: kolom
-- bernama `variant_color` yang isinya "Super Besar" adalah jebakan bagi
-- siapa pun yang membacanya nanti.
ALTER TABLE "products" RENAME COLUMN "variant_color" TO "variant_grade";

-- KATEGORI. Sembilan potongan yang benar-benar dijual toko ini. Ditaruh di
-- migrasi, bukan di seed dev, karena produksi juga membutuhkannya -- dan
-- `db/seed/dev.sql` sengaja tidak pernah ikut ke produksi.
--
-- `ON CONFLICT DO NOTHING` membuatnya aman dijalankan berulang dan tidak
-- menimpa kategori yang mungkin sudah dibuat sendiri lewat halaman produk.
INSERT INTO "categories" ("name") VALUES
    ('Ayam Utuh'), ('Parting'), ('Dada'), ('Paha'), ('Ceker'),
    ('Kulit'), ('Jeroan'), ('Tulang'), ('MDM')
ON CONFLICT ("name") DO NOTHING;

-- Sisa data contoh dari toko lain (lihat db/seed/dev.sql sebelum migrasi
-- ini). Dihapus HANYA kalau belum tersangkut ke mana-mana: kalau ternyata
-- pernah terjual atau masuk tiket, barisnya dibiarkan dan penghapusannya
-- jadi urusan manusia. Dengan penjagaan itu migrasi ini tidak pernah bisa
-- gagal karena foreign key, dan di produksi -- yang memang tidak pernah
-- diberi seed dev -- seluruh blok ini tidak melakukan apa-apa.
DELETE FROM "product_batches"
WHERE product_id IN (
    '33333333-3333-4333-8333-333333333301',
    '33333333-3333-4333-8333-333333333302',
    '33333333-3333-4333-8333-333333333303'
);

DELETE FROM "stock_adjustments"
WHERE product_id IN (
    '33333333-3333-4333-8333-333333333301',
    '33333333-3333-4333-8333-333333333302',
    '33333333-3333-4333-8333-333333333303'
)
AND NOT EXISTS (SELECT 1 FROM transaction_items ti WHERE ti.product_id = stock_adjustments.product_id)
AND NOT EXISTS (SELECT 1 FROM ticket_items tk WHERE tk.product_id = stock_adjustments.product_id)
AND NOT EXISTS (SELECT 1 FROM external_order_items eo WHERE eo.product_id = stock_adjustments.product_id);

DELETE FROM "products" p
WHERE p.id IN (
    '33333333-3333-4333-8333-333333333301',
    '33333333-3333-4333-8333-333333333302',
    '33333333-3333-4333-8333-333333333303'
)
AND NOT EXISTS (SELECT 1 FROM transaction_items ti WHERE ti.product_id = p.id)
AND NOT EXISTS (SELECT 1 FROM ticket_items tk WHERE tk.product_id = p.id)
AND NOT EXISTS (SELECT 1 FROM external_order_items eo WHERE eo.product_id = p.id)
AND NOT EXISTS (SELECT 1 FROM channel_listings cl WHERE cl.product_id = p.id)
AND NOT EXISTS (SELECT 1 FROM shopping_list_items sl WHERE sl.product_id = p.id)
AND NOT EXISTS (SELECT 1 FROM products v WHERE v.parent_id = p.id);

DELETE FROM "categories" c
WHERE c.name IN ('Tas', 'Perlengkapan Minum')
AND NOT EXISTS (SELECT 1 FROM products p WHERE p.category_id = c.id);

-- SATUAN. `products.unit` berhenti dipakai: satu SKU sekarang berarti satu
-- pack, dan satu pack belum tentu satu kilogram. Stok dihitung per pack,
-- jadi "pcs" atau "kg" di sebelah angka stok justru menyesatkan -- isi pack
-- ada di kolom ukuran, bagian dari SKU.
--
-- Kolomnya TIDAK di-drop. Menghapusnya membuang satuan yang mungkin sudah
-- terisi di produksi tanpa bisa dikembalikan, sementara membiarkannya tidak
-- memakan biaya apa pun. Yang berubah: tidak ada lagi kode yang menulis atau
-- membacanya.
COMMENT ON COLUMN "products"."unit" IS
    'Tidak dipakai sejak migrasi 0008. Satu SKU = satu pack; isi pack ada di variant_size.';
