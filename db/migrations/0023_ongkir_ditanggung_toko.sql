-- Diskon umum diganti diskon ongkir: ongkir ditanggung toko jadi beban, selain itu ditagih ke pembeli; uraian di docs/status-proyek.md.

-- Satu penanda, ongkir sebenarnya tetap di `shipping_cost`; baris lama false karena ongkirnya memang ditagih ke pembeli.
ALTER TABLE "transactions"
    ADD COLUMN "shipping_borne_by_store" BOOLEAN NOT NULL DEFAULT false;

-- Turunan di database supaya semua laporan membaca angka yang sama tanpa mengulang CASE; ditagih menambah `total_amount`, subsidi menjadi beban toko.
ALTER TABLE "transactions"
    ADD COLUMN "shipping_charged" DECIMAL(14,2)
        GENERATED ALWAYS AS (CASE WHEN "shipping_borne_by_store" THEN 0 ELSE "shipping_cost" END) STORED,
    ADD COLUMN "shipping_subsidy" DECIMAL(14,2)
        GENERATED ALWAYS AS (CASE WHEN "shipping_borne_by_store" THEN "shipping_cost" ELSE 0 END) STORED;

COMMENT ON COLUMN "transactions"."discount_amount" IS 'Diskon umum lama (migrasi 0015), tak lagi ditulis sejak migrasi 0023; dipertahankan agar omzet transaksi lama tak berubah.';
