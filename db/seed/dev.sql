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

INSERT INTO categories (id, name, created_by) VALUES
  ('22222222-2222-4222-8222-222222222201', 'Tas',        '11111111-1111-4111-8111-111111111101'),
  ('22222222-2222-4222-8222-222222222202', 'Perlengkapan Minum', '11111111-1111-4111-8111-111111111101')
ON CONFLICT (name) DO NOTHING;

INSERT INTO products (id, category_id, name, sku, price, cost_price, stock_qty, low_stock_threshold, unit, created_by) VALUES
  ('33333333-3333-4333-8333-333333333301', '22222222-2222-4222-8222-222222222201',
   'Totebag Kanvas', 'TTB-001', 70000, 45000, 50, 10, 'pcs', '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333302', '22222222-2222-4222-8222-222222222202',
   'Mug Custom', 'MUG-001', 95000, 60000, 30, 5, 'pcs', '11111111-1111-4111-8111-111111111101'),
  ('33333333-3333-4333-8333-333333333303', '22222222-2222-4222-8222-222222222202',
   'Tumbler Stainless', 'TMB-001', 150000, 95000, 8, 10, 'pcs', '11111111-1111-4111-8111-111111111101')
ON CONFLICT (sku) DO NOTHING;

-- Baris platform TikTok Shop. Belum terhubung -- token diisi lewat alur
-- OAuth di /api/platforms/tiktok/connect.
INSERT INTO platforms (id, platform_name, is_connected) VALUES
  ('44444444-4444-4444-8444-444444444401', 'tiktok', false)
ON CONFLICT DO NOTHING;
