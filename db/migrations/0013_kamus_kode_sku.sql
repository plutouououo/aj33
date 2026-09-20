-- Kamus kode pendek untuk perakitan SKU.
--
-- Bentuk SKU sekarang `[JENIS][GRADE]-[MEREK]-[UKURAN]`, maksimal 12
-- karakter termasuk pemisah. Dengan merek selalu 3 huruf dan dua pemisah,
-- jenis + grade + ukuran hanya kebagian 7 karakter -- dan grade berangka
-- seperti "SP 08" (yang harus dibawa utuh supaya tetap terbaca) langsung
-- menghabiskan 4 di antaranya.
--
-- Karena itu inisial bebas saja tidak cukup. Tabel ini menyimpan kode
-- pendek yang dipilih manusia untuk nilai atribut tertentu, dan kode itu
-- menang atas inisial bebas. Kalau sebuah kombinasi tidak muat di 12
-- karakter dan bagiannya belum ada di sini, backend MENOLAK merakit SKU dan
-- menyebut bagian mana yang harus didaftarkan -- bukan memenggal kodenya.
-- `DSP0` untuk grade `SP 08` berhenti menunjuk kelas ukuran yang mana, dan
-- SKU yang tidak bisa dibaca tidak lebih berguna daripada tidak punya SKU.

CREATE TABLE "sku_codes" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    -- Bagian atribut yang dipetakan. Daftarnya dikunci di sini dan harus
    -- sama persis dengan enum `sku::Bagian` di backend.
    "kind" VARCHAR(10) NOT NULL,
    -- Nilai atribut apa adanya, seperti yang diketik pemilik toko
    -- ("SP 08"). Disimpan utuh supaya halaman kamus bisa menampilkannya
    -- kembali dalam bentuk yang dikenali manusia.
    "source" VARCHAR(150) NOT NULL,
    -- Kunci pencarian: `source` tanpa spasi dan tanda baca, huruf besar
    -- semua ("SP08"). Dengan ini "SP 08", "sp08", dan "Sp-08" menemukan
    -- entri yang sama, dan pemiliknya tidak perlu mendaftarkan tiap ejaan
    -- satu per satu. Ditulis backend, bukan diketik.
    "source_key" VARCHAR(150) NOT NULL,
    -- Kode pendeknya. Tanpa pemisah: pemisah di dalam kode satu bagian akan
    -- melahirkan SKU berbagian lebih dari tiga.
    "code" VARCHAR(10) NOT NULL,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "sku_codes_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "sku_codes_kind_check"
        CHECK ("kind" IN ('jenis', 'grade', 'merek', 'ukuran')),
    -- Ditegakkan database, bukan hanya Rust: jalur mana pun yang suatu saat
    -- menulis ke tabel ini -- skrip perbaikan, psql manual, kode baru --
    -- tetap tunduk pada bentuk yang sama. Kode yang memuat huruf kecil atau
    -- tanda baca akan menghasilkan SKU yang melanggar aturannya sendiri.
    CONSTRAINT "sku_codes_code_check" CHECK ("code" ~ '^[A-Z0-9]{1,10}$'),
    CONSTRAINT "sku_codes_source_key_check" CHECK ("source_key" <> '')
);

ALTER TABLE "sku_codes" ADD CONSTRAINT "sku_codes_created_by_fkey"
    FOREIGN KEY ("created_by") REFERENCES "users"("id")
    ON DELETE NO ACTION ON UPDATE NO ACTION;

-- Satu kode per (bagian, nilai). Dua entri untuk nilai yang sama berarti
-- perakitan SKU bergantung pada baris mana yang kebetulan terbaca lebih
-- dulu, dan SKU yang sama bisa lahir berbeda dari hari ke hari.
CREATE UNIQUE INDEX "sku_codes_kind_source_key_key"
    ON "sku_codes" ("kind", "source_key");

-- Kosakata yang sudah dipakai toko.
--
-- MEREK. Nilainya sama dengan yang dihasilkan inisial bebas, dan itu
-- disengaja: SKU yang sudah beredar tidak boleh berubah artinya. Yang
-- berubah cuma tempatnya -- dari aturan di dalam kode jadi baris yang bisa
-- disunting tanpa deploy ulang.
--
-- GRADE SP. Inilah yang benar-benar membutuhkan kamus. Sebagai inisial
-- bebas "SP 08" jadi "SP08" (4 karakter), dan
-- Dada + SP 08 + Best Chicken + 1 kg = DSP08-BEC-1KG sudah 13.
--
-- Kodenya ANGKANYA SAJA, bukan "S08". Dengan "S08" produk toko sendiri
-- masih ada yang tidak muat:
--
--     Ayam Utuh + SP 08 + AFCO + 2 kg  ->  AUS08-AFC-2KG   13  lewat
--                                      ->  AU08-AFC-2KG    12  pas
--
-- Huruf "SP" tidak hilang artinya: posisi kedua di kepala SKU memang milik
-- grade, jadi "AU08" terbaca sebagai Ayam Utuh kelas 08 -- yang persis
-- sebutan yang dipakai di toko.
INSERT INTO "sku_codes" ("kind", "source", "source_key", "code") VALUES
    ('merek',  'AFCO',         'AFCO',        'AFC'),
    ('merek',  'BEST CHICKEN', 'BESTCHICKEN', 'BEC'),
    ('merek',  'OK CHICK',     'OKCHICK',     'OKC'),
    ('grade',  'SP 08',        'SP08',        '08'),
    ('grade',  'SP 09',        'SP09',        '09'),
    ('grade',  'SP 10',        'SP10',        '10')
ON CONFLICT ("kind", "source_key") DO NOTHING;

-- SKU YANG SUDAH ADA TIDAK DISENTUH, dan `products.sku` sengaja TIDAK diberi
-- CHECK constraint panjang 6-12.
--
-- Produk yang lahir sebelum migrasi ini memakai aturan lama yang batasnya
-- 100 karakter; `AUSP08-AFC-2KG` (14) adalah salah satunya. Memaksakan batas
-- baru ke belakang berarti salah satu dari dua hal: migrasi yang gagal di
-- produksi, atau SKU lama yang ditulis ulang -- dan menulis ulang SKU
-- memutus label yang sudah tercetak di pack, listing marketplace yang sudah
-- memetakan kode lama, dan hafalan pegawai. Aturan 6-12 berlaku untuk SKU
-- yang DIRAKIT atau DIKOREKSI sejak sekarang, ditegakkan di `catalog::sku`.
COMMENT ON COLUMN "products"."sku" IS
    'Dirakit backend dari [JENIS][GRADE]-[MEREK]-[UKURAN], 6-12 karakter sejak migrasi 0013. Baris yang lebih lama memakai aturan sebelumnya dan sengaja dibiarkan apa adanya.';
