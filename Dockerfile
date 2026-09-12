# Kontainer backend AJ33.
#
# PENTING: build context-nya adalah AKAR REPO, bukan folder backend/.
# `sqlx::migrate!("../db/migrations")` menanam isi folder migrasi ke dalam
# binary saat compile, jadi `db/` harus ikut terlihat oleh build:
#
#   docker build -t aj33-backend .
#
# Menjalankan `docker build backend/` akan gagal saat kompilasi, dengan pesan
# yang menunjuk makro migrate -- bukan ke penyebab sebenarnya.

# --- Tahap build ------------------------------------------------------
FROM rust:1.90-bookworm AS pembangun

WORKDIR /app

# Cache query sqlx dipakai menggantikan koneksi database saat compile.
# Tanpa ini build butuh Postgres yang hidup, yang tidak ada di dalam builder.
ENV SQLX_OFFLINE=true

# Manifest disalin lebih dulu dan dependensinya dibangun terhadap main.rs
# kosong. Lapisan itu hanya berubah kalau Cargo.toml/Cargo.lock berubah, jadi
# perubahan kode biasa tidak memicu kompilasi ulang seluruh dependensi --
# selisihnya belasan menit pada tiap deploy.
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
RUN mkdir -p backend/src \
    && echo 'fn main() {}' > backend/src/main.rs \
    && cd backend \
    && cargo build --release \
    && rm -rf src target/release/deps/aj33_backend* target/release/aj33-backend

COPY backend/ ./backend/
COPY db/ ./db/

RUN cd backend && cargo build --release

# --- Tahap jalan ------------------------------------------------------
# Runtime cukup image slim: sqlx dan reqwest sama-sama memakai rustls, jadi
# tidak ada OpenSSL yang perlu dibawa. Yang tetap dibutuhkan hanya sertifikat
# root, untuk memverifikasi TLS ke Supabase dan ke API TikTok.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Berjalan sebagai pengguna biasa. Proses ini menghadap internet dan tidak
# pernah perlu menulis ke filesystem -- seluruh state ada di Postgres.
RUN useradd --system --create-home --uid 10001 aj33
USER aj33

COPY --from=pembangun --chown=aj33:aj33 /app/backend/target/release/aj33-backend /usr/local/bin/aj33-backend

# Sekadar penanda; port sebenarnya dibaca dari env PORT saat start, dan
# penyedia hosting biasanya menyetelnya sendiri.
EXPOSE 3000

CMD ["aj33-backend"]
