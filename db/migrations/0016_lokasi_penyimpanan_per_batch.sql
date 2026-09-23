-- Lokasi penyimpanan pindah dari produk ke batch.
--
-- KEADAAN SEBELUM INI. `products.storage_location` (migrasi 0005) adalah
-- satu label rak untuk seluruh barang sebuah produk. Itu benar hanya selama
-- satu produk memang tinggal di satu tempat. Sejak migrasi 0011 stok hidup
-- per batch, dan kiriman yang datang di hari berbeda memang ditaruh di rak
-- yang berbeda: kiriman lama di rak depan supaya cepat keluar, kiriman baru
-- di gudang belakang. Satu label untuk semuanya berarti pengepak dikirim ke
-- rak yang salah untuk sebagian barang, dan label yang kadang benar kadang
-- salah lebih buruk daripada tidak ada label -- ia tetap dipercaya.
--
-- SESUDAH MIGRASI INI lokasi adalah milik batch, sejalan dengan kedaluwarsa
-- dan harga belinya: ketiganya sifat KIRIMAN, bukan sifat barang. Tidak ada
-- lagi "lokasi default" di produk, karena tidak ada lagi tempat yang bisa
-- dijanjikannya dengan benar.
ALTER TABLE "product_batches"
    ADD COLUMN "storage_location" VARCHAR(100);

-- Lokasi yang sudah diisi di produk diturunkan ke SELURUH batch-nya. Ini
-- bukan tebakan: sampai detik sebelum migrasi ini, label produk memang
-- berlaku untuk semua barangnya. Yang berubah sesudahnya adalah masing-
-- masing batch bisa dikoreksi sendiri-sendiri.
UPDATE "product_batches" b
   SET "storage_location" = p."storage_location"
  FROM "products" p
 WHERE p."id" = b."product_id"
   AND p."storage_location" IS NOT NULL;

-- Kolom lama DIBUANG, bukan dibiarkan sebagai cadangan. Dua tempat yang
-- sama-sama menyimpan "di mana barang ini" adalah dua tempat yang cepat
-- atau lambat berselisih, dan yang membaca tidak punya cara tahu mana yang
-- benar. Pembacanya -- tiket packing dan halaman kasir -- ikut dipindahkan
-- ke lokasi batch pada perubahan yang sama.
ALTER TABLE "products" DROP COLUMN "storage_location";
