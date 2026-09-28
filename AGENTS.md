# AGENTS.md

Panduan untuk AI agent yang mengerjakan repo ini. Baca ini dulu sebelum menyentuh kode.

## Apa ini

POS multi-platform untuk satu toko: kasir di tempat, katalog produk dengan stok,
pesanan marketplace (TikTok Shop, Shopee), dan tiket packing untuk pengepak.
Tiga peran: `owner`, `kasir`, `pengepak`.

Ringkasan lengkap keadaan proyek, jebakan yang sudah pernah ditemui, dan
catatan serah-terima: [`docs/status-proyek.md`](docs/status-proyek.md). Baca
itu juga kalau mau paham konteks di balik keputusan desain.

## Arsitektur

```
browser ──443──▶ Caddy ──▶ web :4321 (Astro SSR) ──▶ backend :3000 (Rust) ──▶ Postgres
```

- **backend/** — Rust + Axum 0.8 + sqlx 0.8 (Postgres), JWT HS256, bcrypt
- **web/** — Astro 5 SSR + Tailwind v4. Browser **tidak pernah** memanggil
  backend langsung; semua panggilan API terjadi di server Astro saat
  merender. Sesi disimpan di cookie httpOnly.
- **db/migrations/** — migrasi SQL bernomor urut, dijalankan otomatis oleh
  backend saat start (`sqlx::migrate!`)
- Form pakai POST HTML biasa. JavaScript hanya untuk hal kosmetik (sidebar)
  dan halaman kasir (`web/src/pages/kasir.astro`) — yang **tetap harus
  berfungsi penuh tanpa JS**.

Modul backend (`backend/src/<modul>/{mod,routes,repo,service}.rs`):
`auth`, `catalog` (produk/SKU), `customers`, `marketplace` (Shopee/TikTok),
`orders`, `pos` (kasir), `reports`, `tickets` (packing).

## Konvensi bahasa

Nama domain, komentar, dan teks UI pakai **Bahasa Indonesia** (mengikuti
kode yang sudah ada) — jangan diterjemahkan ke Inggris. Istilah generik dari
Rust/library/framework tetap Inggris seperti biasa.

## Konvensi komentar

- Default: tanpa komentar. Nama yang jelas sudah menjelaskan APA.
- Tulis komentar hanya untuk MENGAPA yang tidak jelas dari kode: invariant
  tersembunyi, aturan bisnis, alasan menghindari pendekatan yang tampak lebih
  jelas, workaround untuk bug/batasan tertentu.
- Komentar yang menjelaskan aturan bisnis non-obvious (mis. perhitungan fee
  Shopee di `backend/src/pos/service.rs`, aturan locking di
  `backend/src/tickets/service.rs`) boleh multi-baris kalau memang perlu.
  Jangan dipangkas jadi satu baris kalau itu menghilangkan informasi penting.
- Komentar basa-basi yang cuma mengulang nama field/fungsi: hapus atau
  padatkan jadi satu baris pendek.

## Menjalankan & menguji

Tidak perlu toolchain Rust lokal — semua `cargo`/`sqlx` dijalankan lewat
Docker via wrapper script (lihat isinya untuk alasan detail):

```bash
docker compose up -d postgres        # Postgres lokal, host port 5433
scripts/cargo.sh run                 # atau: scripts/dev-backend.sh
scripts/cargo.sh test
scripts/cargo.sh clippy -- -D warnings
scripts/sqlx.sh migrate run --source db/migrations
scripts/seed-dev.sh                  # akun contoh: owner/kasir/pengepak
```

Frontend:

```bash
cd web
npm install
npm run dev      # astro dev
npm run check    # type check — wajib lulus, dicek CI
npm run build
```

CI (`.github/workflows/ci.yml`) menjalankan, berurutan:
`cargo fmt --check` → migrasi → `cargo clippy --all-targets -- -D warnings` →
`cargo test` → `cargo sqlx prepare --check` (backend), lalu
`npm run check` → `npm run build` (frontend). Jalankan yang relevan secara
lokal sebelum menganggap perubahan selesai.

## sqlx offline cache

Query `sqlx::query!`/`query_as!` diperiksa saat compile terhadap skema
database, dan cache-nya disimpan di `backend/.sqlx/*.json` (dicommit ke
repo). **Setiap kali mengubah query SQL di kode Rust**, jalankan:

```bash
scripts/sqlx.sh prepare --workspace  # atau lewat scripts/cargo.sh di dalam backend/
```

dan commit file `.sqlx/` yang berubah. Kalau ini terlewat, build produksi
(`SQLX_OFFLINE=true`) memakai bentuk query lama dan CI akan merah di langkah
`cargo sqlx prepare --check`.

## Migrasi database

- File baru di `db/migrations/`, penomoran urut 4 digit
  (`00NN_deskripsi-singkat.sql`), mengikuti pola nama yang sudah ada.
- Backend menjalankan migrasi otomatis saat start — jangan mengedit migrasi
  yang sudah pernah di-commit/deploy, buat migrasi baru sebagai gantinya.

## Deploy

Deploy jalan otomatis lewat `git push` ke `main` (GitHub Actions). Detail
lengkap, termasuk struktur VPS dan perintah operasional (`aj33-rollback`,
`aj33-backup`, dll): [`docs/panduan-deploy.md`](docs/panduan-deploy.md).

## Dokumen lain

- [`docs/panduan-ui.md`](docs/panduan-ui.md) — pola desain/komponen UI
- [`docs/status-proyek.md`](docs/status-proyek.md) — status, keputusan, jebakan
