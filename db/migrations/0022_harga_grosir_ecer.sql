-- Ukuran pack jadi angka kg dan harga ecer/grosir per produk, ambang grosir di pengaturan; uraian lengkap di docs/status-proyek.md.

-- Teks lama diambil angka pertamanya ("2 kg" jadi 2, "0,9kg" jadi 0.9, "500 gr" jadi 0.5); yang tak berangka, nol, atau kelewat panjang menjadi NULL.
ALTER TABLE "products"
    ALTER COLUMN "variant_size" TYPE NUMERIC(8,3)
    USING NULLIF(
        replace(substring("variant_size" from '(?<![\d.,])(\d{1,5}(?:[.,]\d{1,3})?)(?!\d)'), ',', '.')::numeric
            / CASE WHEN "variant_size" ~* '\d\s*(g|gr|gram)\s*$' THEN 1000 ELSE 1 END,
        0
    );

-- Ukuran nol atau negatif tak punya arti sebagai isi pack dan akan membuat berat baris kasir salah hitung.
ALTER TABLE "products" ADD CONSTRAINT "products_variant_size_check"
    CHECK (variant_size IS NULL OR variant_size > 0);

COMMENT ON COLUMN "products"."variant_size" IS 'Isi satu pack dalam kg (boleh pecahan); penentu ecer atau grosir di kasir lewat pricing_settings.';

-- NULL berarti belum diatur dan kasir jatuh ke harga ecer (`price`), bukan gratis, seperti harga kanal di migrasi 0005.
ALTER TABLE "products" ADD COLUMN "price_wholesale" DECIMAL(14,2);

ALTER TABLE "products" ADD CONSTRAINT "products_price_wholesale_check"
    CHECK (price_wholesale IS NULL OR price_wholesale >= 0);

COMMENT ON COLUMN "products"."price" IS 'Harga ecer per pack sekaligus harga dasar kanal toko dan rujukan harga kanal yang kosong.';
COMMENT ON COLUMN "products"."price_wholesale" IS 'Harga grosir per pack, dipakai kanal toko bila berat baris (qty x variant_size) melebihi pricing_settings.wholesale_threshold_kg.';

-- Satu baris saja (kunci boolean selalu true), dibuat di sini supaya pembacaan tak perlu menangani "belum ada baris".
CREATE TABLE "pricing_settings" (
    "id" BOOLEAN NOT NULL DEFAULT true,
    "wholesale_threshold_kg" NUMERIC(8,3) NOT NULL DEFAULT 20,
    "updated_by" UUID,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "pricing_settings_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "pricing_settings_satu_baris_check" CHECK ("id"),
    CONSTRAINT "pricing_settings_threshold_check" CHECK ("wholesale_threshold_kg" > 0),
    CONSTRAINT "pricing_settings_updated_by_fkey" FOREIGN KEY ("updated_by") REFERENCES "users"("id")
);

INSERT INTO "pricing_settings" DEFAULT VALUES;
