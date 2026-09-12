-- Harga per marketplace dan lokasi penyimpanan produk.
--
-- HARGA. Satu produk dijual dengan harga berbeda di tiap kanal: biaya
-- layanan, ongkos kirim yang ditanggung penjual, dan program diskon
-- masing-masing marketplace berbeda, jadi harga yang sama akan menghasilkan
-- margin yang berbeda-beda. `products.price` tetap menjadi harga dasar --
-- itulah yang dipakai kasir di toko, dan itulah rujukan saat harga kanal
-- belum diisi.
--
-- Kolomnya dipatok, bukan tabel harga-per-platform yang genrik. Pilihan ini
-- diambil sadar: toko ini berjualan di dua kanal dan menambah kanal baru
-- adalah peristiwa langka yang memang layak lewat migrasi. Tabel normal
-- akan menambah JOIN di setiap pembacaan produk demi keluwesan yang belum
-- tentu terpakai.
--
-- Keduanya NULL, bukan 0. NULL berarti "belum diatur, pakai harga dasar";
-- 0 berarti "gratis". Membedakan keduanya penting -- kalau NULL diisi 0,
-- produk yang belum diatur harganya akan tampil gratis di marketplace.

-- Tokopedia dan TikTok Shop dihitung satu kanal: sejak TikTok mengakuisisi
-- Tokopedia, TikTok Shop Indonesia berjalan di atas Tokopedia. Dinamai
-- `tiktok` supaya sejalan dengan `platforms.platform_name` yang sudah ada
-- dan dipakai integrasi TikTok Shop Open API.
ALTER TABLE "products" ADD COLUMN "price_shopee" DECIMAL(14,2);
ALTER TABLE "products" ADD COLUMN "price_tiktok" DECIMAL(14,2);

-- Harga tidak boleh negatif. Nilai negatif hanya bisa lahir dari salah
-- ketik, dan akibatnya baru terlihat setelah pesanan masuk.
ALTER TABLE "products" ADD CONSTRAINT "products_price_shopee_check"
    CHECK (price_shopee IS NULL OR price_shopee >= 0);
ALTER TABLE "products" ADD CONSTRAINT "products_price_tiktok_check"
    CHECK (price_tiktok IS NULL OR price_tiktok >= 0);

-- LOKASI PENYIMPANAN. Label rak internal, mis. "Rak A3" atau "Gudang
-- belakang, kotak 12". Dibaca pengepak saat mengerjakan tiket packing.
--
-- Teks bebas, bukan tabel lokasi tersendiri: yang dibutuhkan pengepak
-- hanyalah petunjuk yang bisa dibaca manusia, dan tabel lokasi berarti
-- memelihara daftar rak yang harus diperbarui tiap kali rak digeser.
--
-- JANGAN tertukar dengan `product_stock_locations`. Tabel itu terikat ke
-- `channel_listings` dan menyimpan stok per lokasi gudang MILIK PLATFORM
-- (hasil sinkronisasi dari TikTok/Shopee) -- bukan rak di toko sendiri.
ALTER TABLE "products" ADD COLUMN "storage_location" VARCHAR(100);
