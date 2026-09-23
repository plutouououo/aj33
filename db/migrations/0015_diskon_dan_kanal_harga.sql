-- Diskon dan kanal harga pada transaksi kasir.
--
-- DISKON. Migrasi 0007 menutup pintu "ongkir negatif sebagai diskon" dengan
-- alasan bahwa diskon punya aturannya sendiri dan belum ada di sistem ini.
-- Inilah aturannya.
--
-- Kolom sendiri, BUKAN dikurangkan dari `subtotal`. `subtotal` tetap jumlah
-- baris `transaction_items` -- harga barang dikali jumlahnya -- dan itulah
-- yang membuat struk bisa diperiksa baris demi baris. Kalau diskon dilebur
-- ke sana, tidak ada lagi yang bisa menjawab "potongannya berapa" setelah
-- transaksinya tersimpan, dan jumlah baris struk tidak akan cocok dengan
-- subtotalnya.
--
-- PERINGATAN, meneruskan peringatan migrasi 0007: sejak migrasi ini omzet
-- barang BUKAN lagi `SUM(subtotal)`, melainkan
-- `SUM(subtotal - discount_amount)`. Ongkir tetap di luar keduanya.
ALTER TABLE "transactions"
    ADD COLUMN "discount_amount" DECIMAL(14,2) NOT NULL DEFAULT 0;

-- Dua batas, dan keduanya di database supaya jalur mana pun yang suatu saat
-- menulis ke tabel ini tunduk pada aturan yang sama. Diskon negatif adalah
-- kenaikan harga yang menyamar; diskon melebihi subtotal berarti toko
-- membayar pembeli untuk mengambil barangnya.
ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_discount_check"
    CHECK (discount_amount >= 0 AND discount_amount <= subtotal);

-- KANAL HARGA. Selama Shopee dan Tokopedia belum tersambung ke sistem ini,
-- pesanan dari sana dicatat manual lewat kasir. Harganya bukan harga toko:
-- `products.price_shopee` dan `products.price_tiktok` sudah ada sejak
-- migrasi 0005, tapi tidak ada satu pun jalan untuk memakainya saat menjual.
-- Akibatnya pesanan marketplace tercatat seharga kasir, dan selisihnya
-- muncul sebagai laba yang tidak pernah ada.
--
-- Kanal disimpan, bukan sekadar dipakai sesaat lalu dibuang. Dua alasan:
-- laporan bisa memisahkan penjualan toko dari penjualan marketplace, dan
-- sidik jari `Idempotency-Key` bisa dihitung ulang dari baris yang sudah
-- tersimpan -- tanpa kolom ini, keranjang yang sama yang dikirim ulang
-- setelah kanalnya dibetulkan akan dijawab dengan transaksi lama yang
-- harganya salah, tanpa galat apa pun yang memberi tahu.
--
-- `tiktok` mencakup Tokopedia, mengikuti penamaan `products.price_tiktok`
-- dan `platforms.platform_name`: sejak TikTok mengakuisisi Tokopedia,
-- keduanya satu kanal.
ALTER TABLE "transactions"
    ADD COLUMN "sales_channel" VARCHAR(20) NOT NULL DEFAULT 'toko';

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_sales_channel_check"
    CHECK (sales_channel IN ('toko', 'shopee', 'tiktok'));
