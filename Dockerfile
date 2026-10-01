# Kontainer backend AJ33; build context harus AKAR REPO (`docker build -t aj33-backend .`) karena `sqlx::migrate!` menanam `db/migrations` ke binary.

# --- Tahap build ------------------------------------------------------
FROM rust:1.90-bookworm AS pembangun

WORKDIR /app

# Cache query sqlx menggantikan koneksi database saat compile; tanpanya build butuh Postgres hidup yang tak ada di builder.
ENV SQLX_OFFLINE=true

# Manifest disalin dulu dan dependensi dibangun terhadap main.rs kosong, agar lapisan itu hanya berubah bila Cargo.toml/Cargo.lock berubah dan kode biasa tak memicu kompilasi ulang belasan menit.
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
RUN mkdir -p backend/src \
    && echo 'fn main() {}' > backend/src/main.rs \
    && cd backend \
    && cargo build --release \
    && rm -rf src target/release/deps/aj33_backend* target/release/aj33-backend

COPY backend/ ./backend/
COPY db/ ./db/

RUN cd backend && cargo build --release

# Tahap jalan: image slim cukup karena sqlx dan reqwest memakai rustls (tanpa OpenSSL); hanya sertifikat root yang dibutuhkan untuk TLS ke Supabase dan API TikTok.
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Berjalan sebagai pengguna biasa karena proses menghadap internet dan tak pernah menulis ke filesystem (state di Postgres).
RUN useradd --system --create-home --uid 10001 aj33
USER aj33

COPY --from=pembangun --chown=aj33:aj33 /app/backend/target/release/aj33-backend /usr/local/bin/aj33-backend

# Sekadar penanda; port sebenarnya dari env PORT saat start dan biasanya disetel penyedia hosting.
EXPOSE 3000

CMD ["aj33-backend"]
