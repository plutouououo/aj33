-- Perbaikan 0036: GREATEST mengabaikan NULL sehingga batas produk yang tadinya kosong (ikut bawaan) berubah jadi 1 pack; dikembalikan ke NULL.

-- Batas yang memang bernilai 1 pack sejak sebelum 0036 (Hati B OK CHICK walk in, 1 kg pack 1 kg) hanya bisa dikenali dari id-nya, jadi dipertahankan.
UPDATE "products" SET "wholesale_threshold_qty" = NULL
WHERE "wholesale_threshold_qty" = 1 AND "id" <> 'ee577a5c-8b51-4271-aa84-a8931f7db98d';
UPDATE "products" SET "wholesale_threshold_qty_shopee" = NULL WHERE "wholesale_threshold_qty_shopee" = 1;
UPDATE "products" SET "wholesale_threshold_qty_tiktok" = NULL WHERE "wholesale_threshold_qty_tiktok" = 1;
