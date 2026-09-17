-- Alamat pelanggan, dan indeks yang membuat riwayat belanjanya bisa dibaca.
--
-- ALAMAT. Sejak migrasi 0007 transaksi punya ongkos kirim, artinya sebagian
-- penjualan memang diantar. Alamat tujuannya sampai sekarang tidak punya
-- tempat: yang ada hanya `order_shipping_address`, dan itu milik pesanan
-- marketplace -- terisi dari data platform, satu baris per pesanan, bukan
-- alamat pelanggan toko yang langganan memesan antar.
--
-- TEXT, bukan VARCHAR(n). Alamat di Indonesia tidak punya panjang yang bisa
-- ditebak dengan benar -- "Jl. ... RT 03 RW 05, Kel. ..., Kec. ..., patokan
-- ..." lewat begitu saja dari batas yang kelihatannya longgar. Postgres
-- menyimpan TEXT dan VARCHAR dengan cara yang sama persis, jadi batas yang
-- ditebak-tebak hanya menambah cara untuk gagal.
--
-- Boleh NULL: pembeli yang datang ke toko memang tidak punya alamat antar,
-- dan memaksakan string kosong membuat "tidak diantar" tidak bisa dibedakan
-- dari "alamatnya belum ditanyakan".
ALTER TABLE "customers" ADD COLUMN "address" TEXT;

-- INDEKS FOREIGN KEY. Postgres TIDAK membuat indeks otomatis untuk kolom
-- foreign key -- yang otomatis hanya di sisi kolom yang dirujuk. Akibatnya
-- `WHERE customer_id = $1` memindai seluruh tabel transaksi, dan halaman
-- pelanggan yang menampilkan riwayat belanja satu orang akan melakukannya
-- tiap kali dibuka.
--
-- Parsial: baris tanpa pelanggan (pembeli yang tidak dicatat namanya di
-- kasir) tidak pernah dicari lewat kolom ini, jadi tidak perlu ikut
-- menggemukkan indeks.
CREATE INDEX "idx_transactions_customer_id"
    ON "transactions" ("customer_id")
    WHERE "customer_id" IS NOT NULL;
