#!/usr/bin/env bash
# Mengisi database lokal dengan akun dan produk contoh. Development saja --
# lihat peringatan di db/seed/dev.sql.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
docker compose exec -T postgres psql -U postgres -d bozz -v ON_ERROR_STOP=1 -q < db/seed/dev.sql
echo "Seed dev selesai: owner/owner123, kasir/kasir123, pengepak/pengepak123"
