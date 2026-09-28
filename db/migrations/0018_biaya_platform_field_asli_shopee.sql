-- Menyelaraskan kolom biaya platform Shopee dengan nama field ASLI di API
-- Shopee (`v2.payment.get_escrow_detail`), bukan istilah rakitan sendiri.
--
-- Migrasi 0017 memakai satu kolom "biaya administrasi" untuk mewakili
-- potongan Shopee selain PPh dan biaya proses. Tapi API Shopee yang
-- sesungguhnya memisahkan itu jadi DUA field berbeda:
--
-- - `commission_fee`: "The commission fee charged by Shopee platform if
--   applicable." -- komisi dasar, berlaku untuk semua pesanan.
-- - `service_fee`: "Amount charged by Shopee to seller for additional
--   services." -- biaya program tambahan yang OPSIONAL (mis. Gratis Ongkir
--   Xtra, Star+), nol kalau toko tidak ikut program itu.
--
-- Menggabungkan keduanya jadi satu angka sudah cukup untuk mencatat total
-- potongan, tapi tidak bisa dibandingkan field-demi-field dengan struk asli
-- Shopee (`get_escrow_detail`) kalau toko ini suatu saat tersambung
-- sungguhan. Migrasi ini memisahkannya.
--
-- Dua kolom lain ikut diganti nama supaya konsisten dengan field aslinya:
-- - `platform_pph_fee` -> `platform_withholding_tax`, mengikuti field
--   `withholding_tax` ("According to regulations issued by Directorate
--   General of Taxation in ID, the Withholding Tax is applied to the
--   income stated in the invoice...") -- persis PPh final UMKM yang
--   dimaksud, bukan istilah pajak yang kita karang sendiri.
-- - `platform_processing_fee` -> `platform_order_processing_fee`, mengikuti
--   field `seller_order_processing_fee` ("Order Processing Fee is the
--   amount charged to sellers for every order created.") -- satu-satunya
--   dari ketiganya yang strukturnya sudah cocok sejak awal (Rp1.250 tetap
--   per pesanan), cuma namanya yang belum selaras.
ALTER TABLE "transactions"
    RENAME COLUMN "platform_admin_fee_percent" TO "platform_commission_fee_percent";
ALTER TABLE "transactions"
    RENAME COLUMN "platform_admin_fee" TO "platform_commission_fee";
ALTER TABLE "transactions"
    RENAME COLUMN "platform_pph_fee" TO "platform_withholding_tax";
ALTER TABLE "transactions"
    RENAME COLUMN "platform_processing_fee" TO "platform_order_processing_fee";

-- `service_fee` BENAR-BENAR baru -- migrasi 0017 tidak punya padanannya sama
-- sekali. Default nol: kita tidak tahu toko ini ikut program berbayar
-- Shopee yang mana (atau tidak sama sekali), jadi defaultnya "tidak ada
-- biaya layanan" sampai kasir mengisi sendiri kalau ternyata berlaku.
ALTER TABLE "transactions"
    ADD COLUMN "platform_service_fee_percent" DECIMAL(6,4) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_service_fee" DECIMAL(14,2) NOT NULL DEFAULT 0;

-- Constraint lama menunjuk nama kolom yang sudah tidak ada (PostgreSQL
-- MENULIS ULANG definisinya otomatis saat kolom di-rename, jadi ini murni
-- soal menambahkan kolom service_fee yang baru -- tapi didrop-buat-ulang
-- semuanya sekalian, supaya definisinya kelihatan lengkap di satu tempat,
-- bukan tersebar antara migrasi 0017 dan 0018).
ALTER TABLE "transactions" DROP CONSTRAINT "transactions_platform_fee_check";
ALTER TABLE "transactions" DROP CONSTRAINT "transactions_platform_fee_channel_check";

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_check"
    CHECK (
        platform_commission_fee_percent >= 0 AND platform_commission_fee_percent <= 1
        AND platform_commission_fee >= 0
        AND platform_service_fee_percent >= 0 AND platform_service_fee_percent <= 1
        AND platform_service_fee >= 0
        AND platform_withholding_tax >= 0
        AND platform_order_processing_fee >= 0
    );

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_channel_check"
    CHECK (
        sales_channel = 'shopee'
        OR (
            platform_commission_fee_percent = 0 AND platform_commission_fee = 0
            AND platform_service_fee_percent = 0 AND platform_service_fee = 0
            AND platform_withholding_tax = 0
            AND platform_order_processing_fee = 0
        )
    );
