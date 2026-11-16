-- Nomor pesanan marketplace yang dicatat kasir, supaya pesanan yang sama tak tercatat dua kali (kasir lalu label atau sinkron).

ALTER TABLE "transactions" ADD COLUMN "marketplace_order_no" VARCHAR(100);

-- Transaksi yang sudah di-void tidak menahan nomornya, jadi indeks unik hanya mencakup yang `completed`.
CREATE UNIQUE INDEX "transactions_nomor_marketplace_aktif_key"
    ON "transactions" ("marketplace_order_no")
    WHERE "status" = 'completed' AND "marketplace_order_no" IS NOT NULL;
