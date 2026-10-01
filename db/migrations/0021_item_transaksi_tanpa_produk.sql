-- Produk yang sudah pernah terjual boleh dihapus; item transaksinya tetap ada dengan nama dan harga saat terjual, hanya tautan ke produknya yang dilepas.
ALTER TABLE "transaction_items" ALTER COLUMN "product_id" DROP NOT NULL;
ALTER TABLE "transaction_items" DROP CONSTRAINT "transaction_items_product_id_fkey";
ALTER TABLE "transaction_items" ADD CONSTRAINT "transaction_items_product_id_fkey"
    FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE SET NULL;
