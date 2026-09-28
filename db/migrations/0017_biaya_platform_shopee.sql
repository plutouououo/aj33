-- Biaya platform Shopee, disimpan per transaksi.
--
-- Sebelumnya potongan Shopee (17,25% biaya layanan + 0,5% PPh final UMKM +
-- Rp1.250/pesanan) dihitung ULANG di laporan dengan tarif tetap -- lihat
-- riwayat `backend/src/reports/repo.rs`. Itu berhenti benar sejak biaya
-- administrasinya bisa diedit kasir per transaksi (promo Shopee mengubah
-- persennya dari waktu ke waktu): tarif tetap di laporan tidak lagi mewakili
-- apa yang sungguh dipotong pada transaksi tertentu.
--
-- Sekarang nilainya dihitung SEKALI saat checkout dan disimpan di baris
-- transaksinya sendiri. Laporan tinggal menjumlahkan kolom ini, bukan
-- menghitung ulang dengan tarif global.
--
-- Empat kolom, bukan satu jumlah gabungan: struk dan laporan perlu
-- menunjukkan komponen mana yang berapa, dan `platform_admin_fee_percent`
-- adalah SATU-SATUNYA jejak tarif yang benar-benar dipakai transaksi ini
-- (kasir bisa menggantinya, jadi tidak selalu 17,25%).
ALTER TABLE "transactions"
    ADD COLUMN "platform_admin_fee_percent" DECIMAL(6,4) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_admin_fee" DECIMAL(14,2) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_pph_fee" DECIMAL(14,2) NOT NULL DEFAULT 0,
    ADD COLUMN "platform_processing_fee" DECIMAL(14,2) NOT NULL DEFAULT 0;

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_check"
    CHECK (
        platform_admin_fee_percent >= 0 AND platform_admin_fee_percent <= 1
        AND platform_admin_fee >= 0
        AND platform_pph_fee >= 0
        AND platform_processing_fee >= 0
    );

-- Kanal selain Shopee tidak boleh membawa potongan ini sama sekali. Kalau
-- suatu saat ada jalur lain yang diam-diam mengisi kolom ini untuk kanal
-- toko atau Tokopedia/TikTok, constraint ini yang menolaknya duluan --
-- bukan laporan yang baru ketahuan salah belakangan.
ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_channel_check"
    CHECK (
        sales_channel = 'shopee'
        OR (
            platform_admin_fee_percent = 0
            AND platform_admin_fee = 0
            AND platform_pph_fee = 0
            AND platform_processing_fee = 0
        )
    );
