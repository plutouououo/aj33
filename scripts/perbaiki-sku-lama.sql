-- Menyeragamkan SKU lama ke bentuk baku migrasi 0013.
--
-- Dijalankan SEKALI, lalu file ini boleh dibuang. Bukan migrasi: isinya
-- terikat pada data satu database tertentu pada satu saat tertentu, dan
-- menjalankannya di database lain tidak berarti apa-apa.
--
-- Tiap SKU baru di bawah dihitung oleh `catalog::sku::rakit` yang asli --
-- modulnya dijalankan apa adanya terhadap salinan atribut produksi, bukan
-- ditiru ulang di SQL -- memakai kamus setelah lima entri di bawah dipasang.
-- Karena itu hasilnya identik dengan yang akan dirakit backend kalau produk
-- ini dibuat hari ini.
--
-- DUA HAL YANG DITEMBUS DI SINI, ATAS PERMINTAAN PEMILIK TOKO:
--
-- 1. `koreksi_sku` menolak mengubah SKU produk yang sudah bergerak. Tujuh
--    produk di bawah sudah pernah terjual di kasir dan tetap diubah. Label
--    yang sudah tercetak dan listing marketplace yang memetakan kode lama
--    akan berhenti cocok. Itu diterima sebagai harga penyeragaman.
--
-- 2. Jalur API dilewati sepenuhnya, jadi penjagaan yang tersisa hanya indeks
--    unik `products_sku_key`. Bentuk dan bentrokannya sudah diperiksa di
--    muka; transaksi di bawah menegakkan sisanya.

BEGIN;

-- Kamus dulu, baru SKU. Tanpa lima entri ini ada lima produk yang SKU-nya
-- 13 karakter dan tidak punya bentuk sah mana pun.
--
-- Semuanya memendekkan JENIS, bukan ukuran. Ukuran sengaja dibiarkan utuh
-- ("0.9 kg" -> "09KG"): satuan yang dibuang membuat ukuran berhenti berarti,
-- dan itu alasan yang sama yang ditulis `kode_bebas` untuk tidak memakai
-- inisial pada ukuran.
INSERT INTO sku_codes (kind, source, source_key, code) VALUES
    ('jenis', 'Ayam Utuh Probiotik',    'AYAMUTUHPROBIOTIK',   'AP'),
    ('jenis', 'Hati Jantung Ampela',    'HATIJANTUNGAMPELA',   'HJ'),
    ('jenis', 'Ceker Besar Tanpa Kuku', 'CEKERBESARTANPAKUKU', 'CBT'),
    ('jenis', 'Ceker Kecil Tanpa Kuku', 'CEKERKECILTANPAKUKU', 'CKT'),
    ('jenis', 'Ceker Kecil Ada Kuku',   'CEKERKECILADAKUKU',   'CKA')
ON CONFLICT (kind, source_key) DO NOTHING;

-- SKU. Dikunci pada `id`, bukan pada SKU lama: kalau ada yang menyunting
-- produk yang sama sementara ini berjalan, baris yang salah tidak ikut
-- tersenggol. `updated_at` ditulis tangan karena tabel ini tidak punya
-- trigger -- aplikasinya pun menulisnya sendiri (catalog/repo.rs:452).

-- kulit s paha
--   A-AFC-1KG -> KPA-AFC-1KG
UPDATE products SET sku = 'KPA-AFC-1KG', updated_at = now() WHERE id = 'cd588870-ed8f-44ea-9c34-f10b7d4d49dd';

-- Paha Atas AFCO 14.9
--   A-AFC-1KG-3 -> PAA-AFC-1KG
UPDATE products SET sku = 'PAA-AFC-1KG', updated_at = now() WHERE id = '490319cb-866b-492b-ac5a-5541521444be';

-- Ayam Utuh_SP 06 AFCO 18.8
--   AU -> AUA-AFC-06KG
UPDATE products SET sku = 'AUA-AFC-06KG', updated_at = now() WHERE id = 'dead0311-1863-4658-833d-9fd301154ead';

-- Ayam Utuh_1.0 BEST CHICKEN
--   AU-BEC-1KG -> AUA-BEC-1KG
UPDATE products SET sku = 'AUA-BEC-1KG', updated_at = now() WHERE id = 'e0b429d8-bfeb-4d6f-ab31-1d7efb5933d0';

-- Ayam Utuh_SP 07 AFCO
--   AUA-AFC-1 -> AUA-AFC-07KG
UPDATE products SET sku = 'AUA-AFC-07KG', updated_at = now() WHERE id = 'c0b52ac1-0f5c-4c26-8abd-ebe3ad345c9a';

-- Ayam Utuh Probiotik 0.9 BEST CHICKEN 15.6
--   AYAMUTUHPROBIOTIK09BESTCHICKEN156 -> APA-BEC-09KG
UPDATE products SET sku = 'APA-BEC-09KG', updated_at = now() WHERE id = '473d30f2-d7d6-4af3-aeed-837ab409c800';

-- kulit b ok chick badan potongan kecil
--   B-OKC-1KG -> KBKB-OKC-1KG
UPDATE products SET sku = 'KBKB-OKC-1KG', updated_at = now() WHERE id = '9dd19564-243d-4992-a2e7-7609ac63b6d9';

-- Brutu AFCO_Tunggir 7.9
--   BT-AFC -> BTA-AFC-1KG
UPDATE products SET sku = 'BTA-AFC-1KG', updated_at = now() WHERE id = 'eaed2c7f-f64e-4cc4-8661-f78cba231da4';

-- CBSK Afco Ceker Bersih Ada Kuku Kecil
--   CBA-AFC-1KG -> CKAA-AFC-1KG
UPDATE products SET sku = 'CKAA-AFC-1KG', updated_at = now() WHERE id = 'b6efed0f-995a-4810-b06e-92b7c339d12d';

-- CBSB A Ceker Bersih Super Besar AFCO 16.9
--   CBA-AFC-1KG-2 -> CBTA-AFC-1KG
UPDATE products SET sku = 'CBTA-AFC-1KG', updated_at = now() WHERE id = '06db6764-eaa0-4551-9a21-bd3b0b1e268b';

-- CBSK A Ceker Bersih Ada Kuku Kecil AFCO 16.9
--   CBA-AFC-1KG-3 -> CBAK-AFC-1KG
UPDATE products SET sku = 'CBAK-AFC-1KG', updated_at = now() WHERE id = 'b6f0f82c-7834-4d78-8bdc-8759f9ba0360';

-- CBTK SK Ceker Bersih Tanpa Kuku Kecil
--   CBA-AFC-1KG-4 -> CKTA-AFC-1KG
UPDATE products SET sku = 'CKTA-AFC-1KG', updated_at = now() WHERE id = '0f9f8ecf-c7f2-4696-98c6-4449b81c8e8d';

-- Cincang Dada BEST CHICKEN Pack 2 kg 28.2
--   CINCANGDADABESTCHICKENPACK2KG282 -> CDA-BEC-2KG
UPDATE products SET sku = 'CDA-BEC-2KG', updated_at = now() WHERE id = '83276b1e-6130-4243-9c0f-a6f5c0154591';

-- Ceker Boneless Reguler
--   CTTRA-AFC-1KG -> CBRA-AFC-1KG
UPDATE products SET sku = 'CBRA-AFC-1KG', updated_at = now() WHERE id = '6b86b7a3-3bf8-455f-b66f-7e284bc6e43c';

-- Filet Dada Utuh SBB SB 2.9
--   FILETDADAUTUHSBBSB29 -> FDUA-AFC-1KG
UPDATE products SET sku = 'FDUA-AFC-1KG', updated_at = now() WHERE id = 'f6d18224-c03b-401d-8a8a-0f602aa314e9';

-- filet paha dengan kulit SOBL
--   FILETPAHADENGANKULITSOBL138 -> FPKA-AFC-1KG
UPDATE products SET sku = 'FPKA-AFC-1KG', updated_at = now() WHERE id = '21b7d915-6d67-44e2-816a-429ccd59814d';

-- Hati Jantung Ampela HJA AFCO 18.8
--   HATIJANTUNGAMPELAHJAAFCO188 -> HJA-AFC-09KG
UPDATE products SET sku = 'HJA-AFC-09KG', updated_at = now() WHERE id = '1df2e355-0bda-4b01-8061-7326974c3446';

-- Hati S super AFCO 24.8
--   HATISSUPERAFCO248 -> HSA-AFC-09KG
UPDATE products SET sku = 'HSA-AFC-09KG', updated_at = now() WHERE id = 'b9078f91-92b9-416b-b8bc-cd52527b65c8';

-- Jantung AFCO 10.9
--   JANTUNGAFCO109 -> JA-AFC-09KG
UPDATE products SET sku = 'JA-AFC-09KG', updated_at = now() WHERE id = 'ef246a9b-31ac-4ae0-95af-d4f3837f2e02';

-- kepala best chicken
--   K-BEC-1 -> KA-BEC-1KG
UPDATE products SET sku = 'KA-BEC-1KG', updated_at = now() WHERE id = 'd28c4e10-30ad-4365-b80b-3986c2747d48';

-- kulit s badan lebar
--   KA-AFC-1KG -> KBLA-AFC-1KG
UPDATE products SET sku = 'KBLA-AFC-1KG', updated_at = now() WHERE id = 'b7865b2a-e87a-40ee-a961-a9f79064b17e';

-- kulit leher
--   KB-OKC-1KG -> KLB-OKC-1KG
UPDATE products SET sku = 'KLB-OKC-1KG', updated_at = now() WHERE id = 'f46a193c-cd47-46b0-ac3c-9ae394596182';

-- MDM Zahra Pack 2 kg 10.9
--   MDMZAHRAPACK2KG109 -> MA-ZAH-2KG
UPDATE products SET sku = 'MA-ZAH-2KG', updated_at = now() WHERE id = 'd6c73894-87bf-43d6-bc8f-8f7464960131';

-- MDM AFCO Pack 1 kg 7.9
--   MM-AFC-1 -> MA-AFC-1KG
UPDATE products SET sku = 'MA-AFC-1KG', updated_at = now() WHERE id = '906a42a7-abc7-4951-b497-933e661a8ff1';

-- Paha Bawah BEST CHICKEN 3.6
--   PAHABAWAHBESTCHICKEN36 -> PBA-BEC-1KG
UPDATE products SET sku = 'PBA-BEC-1KG', updated_at = now() WHERE id = 'c9baefbe-33a6-41d0-aed3-e2dd842c9867';

-- Usus AFCO 31.8
--   USUSAFCO318 -> UA-AFC-09KG
UPDATE products SET sku = 'UA-AFC-09KG', updated_at = now() WHERE id = 'dc292589-2a7c-4e3d-9bf7-58927f1a4c2b';

-- Pagar terakhir. Kalau salah satu tidak 26, ada yang bergeser sejak daftar
-- ini disusun dan seluruh transaksi harus batal -- bukan diselesaikan
-- sebagian, karena SKU setengah berubah lebih sulit ditelusuri daripada SKU
-- yang belum disentuh sama sekali.
DO $$
DECLARE n int;
BEGIN
    SELECT count(*) INTO n FROM products
    WHERE sku IN ('KPA-AFC-1KG','PAA-AFC-1KG','AUA-AFC-06KG','AUA-BEC-1KG',
                  'AUA-AFC-07KG','APA-BEC-09KG','KBKB-OKC-1KG','BTA-AFC-1KG',
                  'CKAA-AFC-1KG','CBTA-AFC-1KG','CBAK-AFC-1KG','CKTA-AFC-1KG',
                  'CDA-BEC-2KG','CBRA-AFC-1KG','FDUA-AFC-1KG','FPKA-AFC-1KG',
                  'HJA-AFC-09KG','HSA-AFC-09KG','JA-AFC-09KG','KA-BEC-1KG',
                  'KBLA-AFC-1KG','KLB-OKC-1KG','MA-ZAH-2KG','MA-AFC-1KG',
                  'PBA-BEC-1KG','UA-AFC-09KG');
    IF n <> 26 THEN
        RAISE EXCEPTION 'hanya % dari 26 SKU baru yang terpasang', n;
    END IF;
END $$;

COMMIT;

-- Sesudah COMMIT: sisa yang MASIH melanggar, dan semuanya butuh atribut
-- diisi lebih dulu -- tidak ada yang bisa diperbaiki dari SQL.
SELECT sku, name FROM products
WHERE sku IS NOT NULL
  AND NOT (sku ~ '^[A-Z0-9]+(-[A-Z0-9]+)*$' AND char_length(sku) BETWEEN 6 AND 12)
ORDER BY sku;
