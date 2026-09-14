-- Nama SEO, varian produk, dan batch barang masuk.
--
-- NAMA SEO. `products.name` sejak awal dipakai untuk dua hal sekaligus:
-- nama yang dicari pegawai di kasir dan nama yang tampil di marketplace.
-- Keduanya menuntut hal yang berlawanan -- pegawai butuh nama pendek yang
-- cepat dikenali ("Ayam Fillet 1kg"), sedangkan marketplace butuh judul
-- panjang berisi kata kunci ("Ayam Fillet Dada Tanpa Tulang Segar 1kg
-- Frozen Halal"). Selama satu kolom melayani keduanya, salah satu selalu
-- jadi korban. `seo_name` memisahkannya: `name` tinggal jadi nama
-- identifikasi internal, `seo_name` yang dikirim ke marketplace.
--
-- Boleh NULL: produk yang belum pernah dijual online tidak butuh judul SEO,
-- dan memaksa mengisinya hanya akan melahirkan salinan `name`.
ALTER TABLE "products" ADD COLUMN "seo_name" VARCHAR(255);

-- ATRIBUT PEMBENTUK SKU. SKU tidak lagi diketik manual, melainkan dirakit
-- dari [Merek] - [Jenis Produk] - [Warna] - [Ukuran]. Supaya bisa dirakit
-- ulang saat produk disunting, keempat bagiannya harus tersimpan sebagai
-- kolom tersendiri -- bukan hanya hasil gabungannya. `brand_name` sudah ada
-- sejak 0001 dan dipakai apa adanya sebagai [Merek].
--
-- Warna dan Ukuran adalah sumbu varian: dua baris produk dengan merek dan
-- jenis yang sama tapi warna atau ukuran berbeda adalah dua varian.
ALTER TABLE "products" ADD COLUMN "product_type" VARCHAR(100);
ALTER TABLE "products" ADD COLUMN "variant_color" VARCHAR(60);
ALTER TABLE "products" ADD COLUMN "variant_size" VARCHAR(60);

-- VARIAN. Satu varian adalah baris `products` tersendiri yang menunjuk
-- induknya. Pilihan ini diambil supaya varian ikut mewarisi seluruh mesin
-- yang sudah bekerja di atas `products`: stok terkunci dan tercatat di
-- ledger lewat `stock.rs`, dijual di kasir lewat `transaction_items`,
-- dipetik pengepak lewat `ticket_items`, dan punya batch sendiri. Tabel
-- `product_variants` terpisah akan menuntut semua jalur itu ditulis ulang
-- dengan kunci yang berbeda, demi bentuk data yang sama saja.
--
-- Konsekuensinya: harga tiap varian adalah `price` baris varian itu
-- sendiri, termasuk harga per marketplace-nya. Induk yang punya varian
-- tidak dijual langsung -- yang dijual selalu varian (lihat `scope=sellable`
-- di endpoint daftar produk).
ALTER TABLE "products" ADD COLUMN "parent_id" UUID;

ALTER TABLE "products" ADD CONSTRAINT "products_parent_id_fkey"
    FOREIGN KEY ("parent_id") REFERENCES "products"("id")
    ON DELETE NO ACTION ON UPDATE NO ACTION;

-- Produk tidak boleh jadi induk dirinya sendiri. Kedalaman satu tingkat
-- (varian tidak boleh punya varian) dijaga di aplikasi: aturannya butuh
-- pembacaan baris lain, yang tidak bisa dinyatakan sebagai CHECK.
ALTER TABLE "products" ADD CONSTRAINT "products_parent_id_bukan_diri_sendiri"
    CHECK (parent_id IS NULL OR parent_id <> id);

CREATE INDEX "idx_products_parent" ON "products"("parent_id");

-- BATCH. Tabelnya sudah ada sejak 0001 tapi belum pernah dipakai kode mana
-- pun. Dua penjagaan ditambahkan sekarang, sebelum ada isinya:
--
--   - Jumlah harus positif. Batch adalah catatan barang MASUK; jumlah nol
--     atau negatif tidak punya arti, dan kalau lolos akan menambah stok ke
--     arah yang salah lewat ledger.
--   - Satu nomor batch hanya boleh sekali per produk, supaya barang masuk
--     yang sama tidak tercatat dua kali saat form tak sengaja dikirim
--     ulang. Batch tanpa nomor dibiarkan bebas -- indeks parsial hanya
--     berlaku untuk baris yang nomornya diisi.
ALTER TABLE "product_batches" ADD CONSTRAINT "product_batches_quantity_check"
    CHECK (quantity > 0);

CREATE UNIQUE INDEX "idx_product_batches_produk_nomor"
    ON "product_batches"("product_id", "batch_number")
    WHERE (batch_number IS NOT NULL);
