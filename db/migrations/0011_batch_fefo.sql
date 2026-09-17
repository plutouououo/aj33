-- Batch menjadi sumber stok, bukan sekadar catatan barang masuk.
--
-- KEADAAN SEBELUM INI. `product_batches.quantity` adalah jumlah yang MASUK
-- saat itu dan tidak pernah berkurang; stok berjalan hidup terpisah di
-- `products.stock_qty`. Penjualan hanya menyentuh `stock_qty`, jadi batch
-- dan stok berselisih sejak transaksi pertama: di database pengembangan ada
-- produk berstok 0 yang batch-nya masih tertulis 4. Akibat paling terasa
-- bukan angkanya, melainkan kolom "Kedaluwarsa" di halaman produk: ia
-- memakai MIN(expiry_date) seluruh batch tanpa penyaring, jadi tanggal batch
-- yang barangnya sudah lama habis tetap menyala merah selamanya. Peringatan
-- yang selalu menyala adalah peringatan yang berhenti dibaca.
--
-- SESUDAH MIGRASI INI berlaku satu invarian, dan seluruh kode stok menjaganya:
--
--     products.stock_qty = SUM(product_batches.remaining_qty) per produk
--
-- Karena itu tidak ada lagi "dua angka stok". `stock_qty` tetap ada sebagai
-- ringkasan yang murah dibaca, tapi ia turunan -- bukan sumber kedua.

-- `remaining_qty` DIPISAH DARI `quantity`, bukan menggantinya. `quantity`
-- adalah fakta sejarah ("kiriman ini berisi 10"); menguranginya saat barang
-- terjual akan menghapus fakta itu dan membuat pertanyaan "kiriman kemarin
-- isinya berapa" tidak bisa dijawab lagi.
ALTER TABLE "product_batches" ADD COLUMN "remaining_qty" INTEGER;

-- Baris ledger menyebut batch mana yang dikeluarkan. Inilah yang membuat
-- "yang terjual tadi dari kiriman mana" bisa dijawab, dan tanpanya FEFO
-- hanya bisa dipercaya, tidak bisa diperiksa.
--
-- NULL untuk seluruh baris lama: saat baris-baris itu ditulis, batch memang
-- belum punya hubungan apa pun dengan stok. Dibiarkan NULL, bukan ditebak --
-- tebakan yang tersimpan sebagai data tidak bisa dibedakan dari fakta.
ALTER TABLE "stock_adjustments" ADD COLUMN "batch_id" UUID;

ALTER TABLE "stock_adjustments" ADD CONSTRAINT "stock_adjustments_batch_id_fkey"
    FOREIGN KEY ("batch_id") REFERENCES "product_batches"("id")
    ON DELETE SET NULL ON UPDATE NO ACTION;

-- SALDO AWAL.
--
-- Sisa tiap batch tidak bisa dipulihkan dari riwayat: ledger lama tidak
-- menyebut batch sama sekali. Yang bisa dilakukan hanyalah membagi stok yang
-- ADA SEKARANG ke batch-batch yang ada, dengan satu asumsi yang dinyatakan
-- terang-terangan: penjualan yang sudah lewat dianggap mengambil dari yang
-- kedaluwarsanya paling dekat. Artinya yang TERSISA menumpuk di batch dengan
-- kedaluwarsa paling jauh -- karena itu pembagian di bawah berjalan dari
-- ujung terjauh, mundur.
--
-- Produk yang stoknya melebihi seluruh batch-nya (termasuk produk yang belum
-- pernah punya batch sama sekali) mendapat satu batch "SALDO AWAL" tanpa
-- tanggal kedaluwarsa. Tanpa tanggal, BUKAN ditebak: kalau diberi tanggal
-- karangan, peringatan kedaluwarsa palsu yang justru ingin dihapus migrasi
-- ini akan lahir kembali di hari pertama.
DO $$
DECLARE
    p    RECORD;
    b    RECORD;
    sisa INTEGER;
    ambil INTEGER;
BEGIN
    FOR p IN SELECT id, stock_qty FROM products LOOP
        sisa := GREATEST(p.stock_qty, 0);

        -- Kebalikan urutan FEFO (kedaluwarsa terdekat lebih dulu), jadi
        -- yang terisi duluan adalah batch yang paling belakangan terjual.
        FOR b IN
            SELECT id, quantity FROM product_batches
            WHERE product_id = p.id
            ORDER BY expiry_date DESC NULLS FIRST, received_at DESC, id DESC
        LOOP
            ambil := LEAST(b.quantity, sisa);
            UPDATE product_batches SET remaining_qty = ambil WHERE id = b.id;
            sisa := sisa - ambil;
        END LOOP;

        IF sisa > 0 THEN
            INSERT INTO product_batches
                (product_id, batch_number, quantity, remaining_qty, expiry_date)
            VALUES (p.id, 'SALDO AWAL', sisa, sisa, NULL);
        END IF;
    END LOOP;
END $$;

-- Batch milik produk yang barisnya sudah tidak ada (kalau ada) tetap harus
-- punya nilai sebelum kolomnya dijadikan NOT NULL.
UPDATE "product_batches" SET "remaining_qty" = 0 WHERE "remaining_qty" IS NULL;

ALTER TABLE "product_batches" ALTER COLUMN "remaining_qty" SET NOT NULL;

-- Sisa tidak boleh negatif, dan tidak boleh melebihi isi kiriman. Keduanya
-- di database, bukan hanya di Rust: jalur mana pun yang suatu saat menulis
-- ke tabel ini -- skrip perbaikan, psql manual, kode baru yang lupa lewat
-- `stock.rs` -- tetap tunduk pada batas yang sama.
ALTER TABLE "product_batches" ADD CONSTRAINT "product_batches_remaining_check"
    CHECK (remaining_qty >= 0 AND remaining_qty <= quantity);

-- Indeks untuk pencarian FEFO: urutannya sama persis dengan ORDER BY yang
-- dipakai `stock.rs` saat mengalokasikan, dan parsial pada batch yang masih
-- bersisa -- batch habis tidak pernah jadi kandidat pengeluaran.
CREATE INDEX "idx_product_batches_fefo"
    ON "product_batches" ("product_id", "expiry_date" NULLS LAST, "received_at")
    WHERE "remaining_qty" > 0;

CREATE INDEX "idx_stock_adjustments_batch"
    ON "stock_adjustments" ("batch_id")
    WHERE "batch_id" IS NOT NULL;

-- Invarian diperiksa di sini, bukan dipercaya. Kalau pembagian di atas
-- meleset walau satu butir, migrasi berhenti dan tidak ada yang ter-commit
-- -- jauh lebih baik daripada aplikasi yang menyala di atas stok yang salah.
DO $$
DECLARE
    menyimpang INTEGER;
BEGIN
    SELECT count(*) INTO menyimpang
    FROM products p
    WHERE p.stock_qty <> COALESCE(
        (SELECT SUM(b.remaining_qty) FROM product_batches b WHERE b.product_id = p.id), 0);

    IF menyimpang > 0 THEN
        RAISE EXCEPTION
            'Saldo awal batch tidak cocok dengan stok pada % produk.', menyimpang;
    END IF;
END $$;
