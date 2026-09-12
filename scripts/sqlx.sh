#!/usr/bin/env bash
# Menjalankan sqlx-cli di dalam container. Lihat scripts/cargo.sh untuk
# alasan kenapa lewat Docker.
#
# Contoh:
#   scripts/sqlx.sh migrate run --source db/migrations
set -euo pipefail

export MSYS_NO_PATHCONV=1

cd "$(dirname "${BASH_SOURCE[0]}")/.."
if REPO_ROOT="$(pwd -W 2>/dev/null)"; then :; else REPO_ROOT="$(pwd)"; fi

exec docker run --rm -t \
  --network aj33_default \
  -v "${REPO_ROOT}:/app" \
  -v aj33-sqlx-bin:/sqlx \
  -w /app \
  -e DATABASE_URL="postgresql://postgres:postgres@postgres:5432/bozz" \
  rust:1.90-bookworm \
  /sqlx/bin/sqlx "$@"
