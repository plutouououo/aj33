-- PPh final UMKM juga dicatat pada transaksi kanal toko; komisi, biaya layanan, dan biaya proses pesanan tetap khusus Shopee.
ALTER TABLE "transactions" DROP CONSTRAINT "transactions_platform_fee_channel_check";

ALTER TABLE "transactions"
    ADD CONSTRAINT "transactions_platform_fee_channel_check"
    CHECK (
        sales_channel = 'shopee'
        OR (
            platform_commission_fee_percent = 0 AND platform_commission_fee = 0
            AND platform_service_fee_percent = 0 AND platform_service_fee = 0
            AND platform_order_processing_fee = 0
            AND (platform_withholding_tax = 0 OR sales_channel = 'toko')
        )
    );
