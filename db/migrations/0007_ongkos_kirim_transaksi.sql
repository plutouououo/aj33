-- Ongkos kirim pada transaksi kasir.
--
-- Sebagian penjualan di toko diantar, dan ongkirnya ikut ditagih ke pembeli.
-- Sebelum ini angka itu tidak punya tempat: kasir terpaksa menaikkan harga
-- barang atau menambahkan "produk" fiktif ke keranjang. Keduanya merusak dua
-- hal sekaligus -- laporan penjualan dan catatan stok.
--
-- KOLOM SENDIRI, BUKAN DILEBUR KE `subtotal`. `subtotal` adalah harga barang,
-- dan itulah yang dibandingkan dengan harga pokok untuk menghitung margin.
-- Ongkir bukan barang dan tidak punya margin; kalau ikut masuk subtotal,
-- setiap perhitungan margin jadi salah tanpa ada yang menyadarinya. Yang
-- bertambah hanya `total_amount` -- jumlah yang benar-benar dibayar pembeli,
-- dan itu pula yang dipakai memeriksa kecukupan uang tunai serta menghitung
-- kembalian.
--
-- PERINGATAN untuk laporan yang belum ditulis: sejak migrasi ini,
-- `SUM(total_amount)` BUKAN lagi omzet barang. Omzet barang adalah
-- `SUM(subtotal)`.
--
-- NOT NULL DEFAULT 0, bukan NULL. Berbeda dari harga marketplace di migrasi
-- 0005 yang membedakan "belum diatur" dari "gratis", di sini tidak ada yang
-- perlu dibedakan: transaksi tanpa pengantaran memang berongkir nol, dan
-- seluruh baris lama memang begitu.
ALTER TABLE "transactions"
    ADD COLUMN "shipping_cost" DECIMAL(14,2) NOT NULL DEFAULT 0;

-- Ongkir negatif adalah diskon yang menyamar. Diskon punya aturannya sendiri
-- dan belum ada di sistem ini, jadi jalan pintasnya ditutup sejak awal.
ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_shipping_cost_check"
    CHECK (shipping_cost >= 0);
