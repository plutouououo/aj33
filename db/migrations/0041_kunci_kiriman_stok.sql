-- Kunci sekali-pakai untuk Catat batch dan Koreksi stok: baris kunci ditulis dalam transaksi yang sama dengan perubahan stok, jadi kiriman ulang tak pernah mencatat dua kali.

CREATE TABLE "stock_request_keys" (
    "key"        VARCHAR(100) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "stock_request_keys_pkey" PRIMARY KEY ("key")
);

ALTER TABLE "stock_request_keys" ENABLE ROW LEVEL SECURITY;
