#!/usr/bin/env bash
# Menjalankan cargo di container agar tak butuh toolchain Rust + MSVC; contoh: `scripts/cargo.sh test`, `scripts/cargo.sh clippy -- -D warnings`.
set -euo pipefail

# Git Bash di Windows menerjemahkan path Unix di argumen docker dan merusaknya; MSYS_NO_PATHCONV mematikannya dan `pwd -W` memberi path Windows yang dimengerti Docker Desktop.
export MSYS_NO_PATHCONV=1

cd "$(dirname "${BASH_SOURCE[0]}")/.."
if REPO_ROOT="$(pwd -W 2>/dev/null)"; then :; else REPO_ROOT="$(pwd)"; fi

exec docker run --rm -t \
  --network aj33_default \
  -v "${REPO_ROOT}:/app" \
  -v aj33-cargo-registry:/usr/local/cargo/registry \
  -v aj33-cargo-target:/app/backend/target \
  -w /app/backend \
  -e CARGO_TERM_COLOR=always \
  -e DATABASE_URL="postgresql://postgres:postgres@postgres:5432/aj33" \
  rust:1.90-bookworm \
  cargo "$@"
