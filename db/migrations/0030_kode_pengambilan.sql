-- Kode pengambilan dari label Shopee (mis. "9WPS") yang ditampilkan di layar packing; nomor pesanan tetap di external_order_id untuk laporan.
ALTER TABLE "external_orders" ADD COLUMN "pickup_code" VARCHAR(20);
