-- Harga beli menjadi milik batch, bukan produk keseluruhan.
--
-- Keputusan bisnis final: `products.price` tetap satu harga jual default,
-- sedangkan harga modal dihitung dari batch sumber barang yang keluar.
-- Karena itu setiap kiriman barang masuk harus bisa mencatat harga belinya
-- sendiri, dan laporan laba mengambil harga dari batch yang terjual.

ALTER TABLE "product_batches"
    ADD COLUMN "purchase_price" DECIMAL(14,2);

-- Data lama tidak punya harga beli per batch, dan itu bukan kesalahan yang
-- bisa ditebak otomatis. Biarkan NULL agar laporan laba dapat memperingatkan
-- data yang belum dilengkapi, bukan mengasumsikan angka yang tidak kita
-- ketahui.
ALTER TABLE "product_batches"
    ADD CONSTRAINT "product_batches_purchase_price_check"
    CHECK (purchase_price IS NULL OR purchase_price >= 0);
