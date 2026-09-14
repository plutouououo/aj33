-- Data awal untuk DEVELOPMENT saja. Sengaja BUKAN migrasi: kalau ikut
-- terdaftar di db/migrations/, akun-akun di bawah beserta password yang
-- tertulis terang di file ini akan ikut terpasang di produksi.
--
-- Aman dijalankan berulang (ON CONFLICT).
--
-- Cara pakai:  scripts/seed-dev.sh
-- owner    / owner123
-- kasir    / kasir123
-- pengepak / pengepak123
INSERT INTO users (id, name, email_or_username, password_hash, role, is_active) VALUES
  ('11111111-1111-4111-8111-111111111101', 'Owner Toko',  'owner',
   '$2b$10$0jnyG.ADvci/7Tjwhnwgl.EmrZlA0lGAqBgUQ5E6ANvcTlbcZiYEK', 'owner', true),
  ('11111111-1111-4111-8111-111111111102', 'Kasir Depan', 'kasir',
   '$2b$10$aj3ZKJhXLGop0TPQlBbnpu1pzd7HyUgnLdaoEol0/0ytzdpOV6fH2', 'kasir', true),
  ('11111111-1111-4111-8111-111111111103', 'Pengepak Gudang', 'pengepak',
   '$2b$10$YcDCU.O9FMSVZE3ss2YfmeJNNIT.rjd8TFdSjJoHO8z4slFQ1WUii', 'pengepak', true)
ON CONFLICT (email_or_username) DO NOTHING;

-- Kategori sudah dipasang migrasi 0008 (sembilan potongan ayam), jadi tidak
-- diulang di sini. Yang di bawah cuma contoh produk supaya halaman tidak
-- kosong saat development.
--
-- SKU-nya sengaja ditulis sama dengan yang dirakit `catalog::sku`, supaya
-- data contoh tidak mengajarkan bentuk yang salah.
INSERT INTO products
  (id, category_id, name, seo_name, sku, product_type, variant_grade, brand_name,
   variant_size, price, cost_price, stock_qty, low_stock_threshold, created_by)
SELECT
  v.id, c.id, v.name, v.seo_name, v.sku, v.jenis, v.grade, v.merek, v.ukuran,
  v.price, v.cost_price, v.stock, 5, '11111111-1111-4111-8111-111111111101'
FROM (VALUES
  ('33333333-3333-4333-8333-333333333311'::uuid, 'Ceker',
   'CBSB ceker bersih super besar',
   'Ceker Ayam Bersih Super Besar Frozen 2kg Halal',
   'CBSB-AFC-2KG', 'Ceker Bersih', 'Super Besar', 'AFCO', '2 kg',
   28000::numeric, 21000::numeric, 24),
  ('33333333-3333-4333-8333-333333333312'::uuid, 'Jeroan',
   'HJA hati jantung ampela',
   'Hati Jantung Ampela Ayam Segar Frozen 1kg',
   'HJA-AFC-1KG', 'Hati Jantung Ampela', NULL, 'AFCO', '1 kg',
   19500::numeric, 14000::numeric, 16),
  ('33333333-3333-4333-8333-333333333313'::uuid, 'Ayam Utuh',
   'Ayam utuh SP 08',
   'Ayam Broiler Utuh Karkas 0.8kg Frozen Halal',
   'AUSP08-AFC-2KG', 'Ayam Utuh', 'SP 08', 'AFCO', '2 kg',
   52000::numeric, 41000::numeric, 10),
  ('33333333-3333-4333-8333-333333333314'::uuid, 'Dada',
   'Filet dada utuh pack 2 kg',
   'Filet Dada Ayam Tanpa Tulang Tanpa Kulit 2kg Frozen',
   'FDU-BEC-2KG', 'Filet Dada Utuh', NULL, 'BEST CHICKEN', '2 kg',
   78000::numeric, 63000::numeric, 6)
) AS v(id, kategori, name, seo_name, sku, jenis, grade, merek, ukuran, price, cost_price, stock)
JOIN categories c ON c.name = v.kategori
ON CONFLICT (sku) DO NOTHING;

-- Dua batch untuk satu produk yang sama: inilah yang dulu dikerjakan dengan
-- menuliskan tanggal kedaluwarsa di ekor nama produk ("... afco 25.7" dan
-- "... afco 29.6" adalah barang yang sama, bukan dua barang).
INSERT INTO product_batches (product_id, batch_number, quantity, expiry_date, created_by)
VALUES
  ('33333333-3333-4333-8333-333333333313', 'SP08-2507', 6, '2027-07-25',
   '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333313', 'SP08-2906', 4, '2027-06-29',
   '11111111-1111-4111-8111-111111111101')
ON CONFLICT DO NOTHING;

-- Baris platform TikTok Shop. Belum terhubung -- token diisi lewat alur
-- OAuth di /api/platforms/tiktok/connect.
INSERT INTO platforms (id, platform_name, is_connected) VALUES
  ('44444444-4444-4444-8444-444444444401', 'tiktok', false)
ON CONFLICT DO NOTHING;
