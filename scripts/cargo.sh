#!/usr/bin/env bash
# Menjalankan cargo di dalam container, supaya mesin ini tidak perlu
# toolchain Rust + MSVC build tools. Registry crate dan direktori target
# disimpan di volume Docker agar build kedua dan seterusnya tetap cepat.
#
# Contoh:
#   scripts/cargo.sh build
#   scripts/cargo.sh test
#   scripts/cargo.sh clippy -- -D warnings
set -euo pipefail

# Git Bash di Windows menerjemahkan path gaya Unix di argumen docker menjadi
# path Windows dan merusaknya. MSYS_NO_PATHCONV mematikan penerjemahan itu,
# dan `pwd -W` memberi path Windows yang memang dimengerti Docker Desktop.
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
