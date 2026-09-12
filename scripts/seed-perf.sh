#!/usr/bin/env bash
# Mengisi katalog dengan produk buatan untuk menguji performa daftar produk.
# Development saja.
#
# Semua baris yang dibuat di sini ber-SKU "PERF-xxxxxx", jadi bisa dibedakan
# dari produk seed biasa dan dihapus kembali tanpa menyentuh yang lain.
#
#   scripts/seed-perf.sh [jumlah]   tambah produk (bawaan 5000)
#   scripts/seed-perf.sh --bersihkan   hapus produk PERF- yang belum terpakai
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

psql() {
  docker compose exec -T postgres psql -U postgres -d aj33 -v ON_ERROR_STOP=1 -q "$@"
}

if [[ "${1:-}" == "--bersihkan" ]]; then
  # Produk yang sudah pernah terjual atau masuk tiket tidak dihapus:
  # foreign key-nya menahan, dan riwayat transaksi tidak boleh kehilangan
  # produk yang diacunya. Sisanya dibuang.
  psql <<'SQL'
DELETE FROM products p
WHERE p.sku LIKE 'PERF-%'
  AND NOT EXISTS (SELECT 1 FROM transaction_items t WHERE t.product_id = p.id)
  AND NOT EXISTS (SELECT 1 FROM ticket_items t WHERE t.product_id = p.id)
  AND NOT EXISTS (SELECT 1 FROM external_order_items i WHERE i.product_id = p.id)
  AND NOT EXISTS (SELECT 1 FROM stock_adjustments s WHERE s.product_id = p.id);

SELECT count(*) AS "produk PERF- tersisa (terpakai transaksi)"
FROM products WHERE sku LIKE 'PERF-%';
SQL
  exit 0
fi

JUMLAH="${1:-5000}"
if ! [[ "$JUMLAH" =~ ^[0-9]+$ ]] || [[ "$JUMLAH" -lt 1 ]]; then
  echo "Jumlah harus bilangan bulat positif, bukan '$JUMLAH'." >&2
  exit 1
fi

# Nama dirakit dari tiga daftar kata supaya pencarian ILIKE menemukan jumlah
# hasil yang berbeda-beda per kata kunci -- daftar produk yang semuanya
# bernama sama tidak menguji apa pun.
psql -v jumlah="$JUMLAH" <<'SQL'
WITH kat AS (
  SELECT array_agg(id ORDER BY name) AS ids FROM categories
),
kata AS (
  SELECT
    ARRAY['Totebag','Mug','Tumbler','Tas Ransel','Botol','Gelas','Piring',
          'Topi','Kaos','Dompet','Sarung Tangan','Payung'] AS jenis,
    ARRAY['Kanvas','Stainless','Keramik','Plastik','Kulit','Bambu','Katun'] AS bahan,
    ARRAY['Hitam','Putih','Merah','Biru','Hijau','Kuning','Cokelat','Abu'] AS warna
),
baris AS (
  SELECT
    i,
    -- Nomor urut lanjut dari baris PERF- yang sudah ada, supaya menjalankan
    -- skrip ini dua kali menambah produk, bukan gagal karena SKU bentrok.
    i + COALESCE((SELECT max(substring(sku FROM 6)::int) FROM products
                  WHERE sku ~ '^PERF-[0-9]+$'), 0) AS urut
  FROM generate_series(1, :jumlah) AS i
)
INSERT INTO products (category_id, name, sku, price, cost_price, stock_qty,
                      low_stock_threshold, unit, is_active)
SELECT
  CASE WHEN k.ids IS NULL THEN NULL
       ELSE k.ids[1 + (b.i % array_length(k.ids, 1))] END,
  w.jenis[1 + (b.i % array_length(w.jenis, 1))] || ' ' ||
    w.bahan[1 + ((b.i / 12) % array_length(w.bahan, 1))] || ' ' ||
    w.warna[1 + ((b.i / 84) % array_length(w.warna, 1))] || ' ' || b.urut,
  'PERF-' || lpad(b.urut::text, 6, '0'),
  harga.nilai,
  round(harga.nilai * 0.6, 2),
  b.i % 200,
  5,
  'pcs',
  -- Sebagian kecil dinonaktifkan: daftar produk menyaring is_active, dan
  -- katalog yang semuanya aktif menyembunyikan biaya penyaringan itu.
  b.i % 50 <> 0
FROM baris b
CROSS JOIN kat k
CROSS JOIN kata w
CROSS JOIN LATERAL (SELECT 5000 + ((b.i * 137) % 495000) / 500 * 500 AS nilai) harga;

SELECT count(*) FILTER (WHERE sku LIKE 'PERF-%') AS "produk perf",
       count(*) AS "total produk",
       count(*) FILTER (WHERE is_active) AS "aktif"
FROM products;
SQL

echo "Selesai. Hapus lagi dengan: scripts/seed-perf.sh --bersihkan"
