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
--
-- Akun `retno` TIDAK ada di sini: ia dipasang migrasi 0012 karena memang
-- harus ikut ke produksi. Password awalnya retno123, dan login pertamanya
-- langsung diminta menggantinya.
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
-- SKU-nya sengaja ditulis sama dengan yang dirakit `catalog::sku` memakai
-- kamus yang dipasang migrasi 0013, supaya data contoh tidak mengajarkan
-- bentuk yang salah. Keempatnya 6-12 karakter:
--
--     CBSB-AFC-2KG  12   Ceker Bersih + Super Besar + AFCO + 2 kg
--     HJA-AFC-1KG   11   Hati Jantung Ampela + AFCO + 1 kg
--     AU08-AFC-2KG  12   Ayam Utuh + SP 08 (kamus: 08) + AFCO + 2 kg
--     FDU-BEC-2KG   11   Filet Dada Utuh + BEST CHICKEN + 2 kg
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
   'AU08-AFC-2KG', 'Ayam Utuh', 'SP 08', 'AFCO', '2 kg',
   52000::numeric, 41000::numeric, 10),
  ('33333333-3333-4333-8333-333333333314'::uuid, 'Dada',
   'Filet dada utuh pack 2 kg',
   'Filet Dada Ayam Tanpa Tulang Tanpa Kulit 2kg Frozen',
   'FDU-BEC-2KG', 'Filet Dada Utuh', NULL, 'BEST CHICKEN', '2 kg',
   78000::numeric, 63000::numeric, 6)
) AS v(id, kategori, name, seo_name, sku, jenis, grade, merek, ukuran, price, cost_price, stock)
JOIN categories c ON c.name = v.kategori
ON CONFLICT (sku) DO NOTHING;

-- Batch untuk SETIAP produk di atas, dan jumlahnya harus sama dengan
-- `stock_qty` produknya.
--
-- Sejak migrasi 0011 berlaku invarian `products.stock_qty =
-- SUM(product_batches.remaining_qty)`, dan sejak kasir memilih batch sendiri,
-- produk tanpa batch tidak bisa dijual sama sekali -- tidak ada yang bisa
-- dipilih. Data contoh yang stoknya menggantung tanpa batch karena itu bukan
-- sekadar tidak rapi: ia mengajarkan keadaan yang tidak mungkin ada.
--
-- `remaining_qty` = `quantity`: kiriman yang baru datang belum ada yang
-- keluar. Kolomnya wajib diisi (NOT NULL sejak 0011) dan tidak punya default.
--
-- Produk ...313 sengaja punya DUA batch: itulah yang dulu dikerjakan dengan
-- menuliskan tanggal kedaluwarsa di ekor nama produk ("... afco 25.7" dan
-- "... afco 29.6" adalah barang yang sama, bukan dua barang), sekaligus satu-
-- satunya cara menguji penjualan yang mengambil dari dua kiriman sekaligus.
INSERT INTO product_batches
  (product_id, batch_number, quantity, remaining_qty, expiry_date, created_by)
VALUES
  ('33333333-3333-4333-8333-333333333311', 'CBSB-2508', 24, 24, '2027-08-25',
   '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333312', 'HJA-2511', 16, 16, '2027-11-30',
   '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333313', 'SP08-2507', 6, 6, '2027-07-25',
   '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333313', 'SP08-2906', 4, 4, '2027-06-29',
   '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333314', 'FDU-2604', 6, 6, '2027-04-30',
   '11111111-1111-4111-8111-111111111101')
ON CONFLICT DO NOTHING;

-- Baris platform marketplace. Keduanya belum terhubung -- token diisi lewat
-- alur OAuth di /api/platforms/{tiktok,shopee}/connect.
--
-- Barisnya harus ada sebelum order pertama masuk: `external_orders`
-- menunjuk ke `platforms` lewat foreign key, jadi order yang tiba sementara
-- barisnya belum ada akan ditolak database, bukan sekadar gagal dipetakan.
INSERT INTO platforms (id, platform_name, is_connected) VALUES
  ('44444444-4444-4444-8444-444444444401', 'tiktok', false),
  ('44444444-4444-4444-8444-444444444402', 'shopee', false)
ON CONFLICT DO NOTHING;
