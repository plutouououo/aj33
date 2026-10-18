-- Pesanan Shopee dibaca dari PDF label pengiriman (API Shopee tak bisa dipakai), dikirim sebagai tiket ke pengepak, dan dicatat sebagai penjualan saat tiket selesai.

-- Label menyebut kabupaten/kota dan kecamatan di kotak tersendiri; dipisah dari `address` agar pelanggan bisa dikelompokkan per wilayah.
ALTER TABLE "customers" ADD COLUMN "city" VARCHAR(100);
ALTER TABLE "customers" ADD COLUMN "district" VARCHAR(100);

-- Batch yang dipilih owner saat mengirim tiket; NULL berarti FEFO saat tiket selesai.
ALTER TABLE "ticket_items" ADD COLUMN "batch_id" UUID REFERENCES "product_batches"("id");

-- Penjualan yang tercatat saat tiket pesanan ini selesai; NULL selama barang belum keluar.
ALTER TABLE "external_orders" ADD COLUMN "transaction_id" UUID REFERENCES "transactions"("id");

-- Baris platform Shopee selama ini baru lahir saat OAuth tersambung, padahal pesanan dari label tetap butuh platform_id.
INSERT INTO "platforms" ("platform_name") VALUES ('shopee')
ON CONFLICT ("platform_name") DO NOTHING;

-- Langganan Web Push per perangkat; satu pengguna bisa punya beberapa perangkat, endpoint unik per perangkat.
CREATE TABLE "push_subscriptions" (
    "id" UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    "user_id" UUID NOT NULL REFERENCES "users"("id") ON DELETE CASCADE,
    "endpoint" TEXT NOT NULL UNIQUE,
    "p256dh" TEXT NOT NULL,
    "auth" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX "push_subscriptions_user_id_idx" ON "push_subscriptions" ("user_id");

-- Defense-in-depth inert seperti tabel lain (lihat 0002 dan docs/panduan-deploy.md §3).
ALTER TABLE "push_subscriptions" ENABLE ROW LEVEL SECURITY;
