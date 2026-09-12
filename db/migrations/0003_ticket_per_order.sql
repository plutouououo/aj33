-- Satu pesanan marketplace hanya boleh punya satu tiket packing.
--
-- Tanpa indeks ini, dua permintaan "buat tiket" yang datang bersamaan untuk
-- pesanan yang sama menghasilkan dua tiket -- dan karena stok dikurangi saat
-- serah-terima, barangnya akan berkurang dua kali. Aturan ini ditegakkan di
-- database, bukan hanya di kode, supaya tidak bergantung pada urutan yang
-- kebetulan. Query daftar pesanan juga bersandar padanya: LEFT JOIN ke
-- tickets mengandaikan paling banyak satu baris pasangan.
CREATE UNIQUE INDEX "idx_tickets_external_order" ON "tickets"("external_order_id");
