#!/usr/bin/env bash
# Menjalankan backend di dalam container, terhubung ke Postgres milik
# docker-compose. Setelan diambil dari backend/.env, kecuali DATABASE_URL
# yang harus menunjuk ke nama service `postgres` (bukan localhost:5433,
# yang hanya berlaku dari sisi host).
set -euo pipefail

export MSYS_NO_PATHCONV=1

cd "$(dirname "${BASH_SOURCE[0]}")/.."
if REPO_ROOT="$(pwd -W 2>/dev/null)"; then :; else REPO_ROOT="$(pwd)"; fi

exec docker run --rm -t \
  --name aj33-backend-dev \
  --network aj33_default \
  -p 3000:3000 \
  -v "${REPO_ROOT}:/app" \
  -v aj33-cargo-registry:/usr/local/cargo/registry \
  -v aj33-cargo-target:/app/backend/target \
  -w /app/backend \
  --env-file backend/.env \
  -e DATABASE_URL="postgresql://postgres:postgres@postgres:5432/aj33" \
  -e CARGO_TERM_COLOR=always \
  rust:1.90-bookworm \
  cargo run
