-- Transaksi void yang pelanggannya dihapus tetap menyimpan namanya di sini; kolom ini hanya diisi saat tautan `customer_id` dilepas.
ALTER TABLE "transactions"
    ADD COLUMN "customer_name_snapshot" VARCHAR(150);
