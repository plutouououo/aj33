# Panduan Deploy & Menghubungkan ke Supabase

Susunan yang dipakai — semuanya tier gratis:

```
browser ──▶ Vercel (web, Astro SSR) ──▶ Render (backend, Rust/Axum) ──▶ Supabase (Postgres)
              Hobby                        Free                           Free
```

Tiga penyedia, tiga proses. **Pilih region yang sama untuk ketiganya**
(Singapura, yang terdekat dari Indonesia). Tiap render halaman melewati
ketiganya berurutan, jadi kalau tersebar di benua berbeda, setiap klik menu
membayar perjalanan bolak-balik yang tidak perlu.

## Dua hal yang perlu diketahui sejak awal

**Backend terbuka ke internet, dan itu bisa diterima.** Browser tidak pernah
memanggil backend langsung — semua permintaan API terjadi di dalam server
Astro saat merender (`web/src/lib/api.ts`). Tapi karena web di Vercel dan
backend di Render, keduanya tidak bisa satu jaringan privat, jadi backend
harus punya alamat publik. Ini aman karena dari seluruh endpoint, hanya empat
yang tidak menuntut token login: `POST /api/auth/login`, `GET /api/health`,
dan dua endpoint TikTok (`/platforms/tiktok/callback` dan `/webhooks/tiktok`,
yang diverifikasi tanda tangan). Semua endpoint produk, kasir, pesanan, dan
tiket menolak permintaan tanpa JWT.

Yang belum ada pengamanannya: **rate limit di endpoint login**. Lihat §9.

**Vercel Hobby melarang penggunaan komersial.** Fair Use Guidelines mereka
(per 29 Juli 2026) menyebut Hobby "restricted to non-commercial personal use
only", dengan definisi komersial mencakup *"any method of requesting or
processing payment from visitors of the site"*. POS toko masuk definisi itu.
Panduan ini tetap memakai Hobby karena itu keputusan pemilik proyek, tapi
risikonya nyata: akun bisa diminta upgrade atau dihentikan. Kalau suatu saat
pindah, bagian §6 yang perlu diganti — sisanya tetap.

---

## 1. Menyiapkan database Supabase

### 1.1 Ambil connection string yang benar

Di dashboard Supabase: **Project → Connect**. Ada tiga pilihan, dan hanya satu
yang tepat untuk backend ini.

| Pilihan | Port | Pakai? |
| --- | --- | --- |
| **Session pooler** | 5432 | ✅ **Ini yang dipakai** |
| Direct connection | 5432 | Hanya kalau host punya IPv6. Render tidak menjamin itu |
| Transaction pooler | 6543 | ❌ **Jangan** |

**Kenapa bukan transaction pooler.** Port 6543 adalah PgBouncer mode
*transaction*, yang tidak mendukung prepared statement. sqlx — yang dipakai
backend ini — menyiapkan tiap query sebagai prepared statement. Sambungannya
akan hidup, lalu query mulai gagal tidak beraturan dengan galat semacam
`prepared statement "sqlx_s_1" already exists` begitu ada dua permintaan
bersamaan. Session pooler memegang satu koneksi Postgres per klien, jadi
prepared statement bertahan sebagaimana mestinya.

```
postgresql://postgres.<project-ref>:<PASSWORD>@aws-0-<region>.pooler.supabase.com:5432/postgres?sslmode=require
```

Username-nya `postgres.<project-ref>`, bukan `postgres` saja — itu khas pooler
Supabase. Kalau password berisi karakter non-alfanumerik, URL-encode dulu
(`@` → `%40`, `#` → `%23`, dan seterusnya).

### 1.2 Role-nya harus `postgres`

> **Ini yang paling mudah salah dan paling sulit didiagnosis.**

`db/migrations/0002_rls_and_checks.sql` menyalakan Row Level Security di 30
tabel **tanpa satu pun policy**. Artinya hanya role pemilik tabel yang bisa
membaca isinya. Kalau `DATABASE_URL` memakai role lain, backend tetap hidup,
health check tetap hijau, login tetap memanggil database — dan **semua query
mengembalikan nol baris tanpa pesan error apa pun**. Gejalanya: login gagal
terus dengan "Username atau password salah" padahal datanya jelas ada.

Pakai role `postgres` (yang menjalankan migrasi, sehingga memiliki tabelnya).
Jangan `anon`, `authenticated`, atau `service_role` — itu role untuk
PostgREST, bukan untuk koneksi Postgres langsung.

Peringatan yang sama ada di `backend/src/db.rs`.

### 1.3 Batas tier gratis yang relevan

| | Free |
| --- | --- |
| Ukuran database | 500 MB |
| Egress | 5 GB + 5 GB cached |
| Koneksi | 60 langsung, **200 lewat pooler** |
| Backup otomatis | **Tidak ada** |
| Project aktif | 2 (yang terjeda tidak dihitung) |

Pool backend disetel 10 koneksi (`backend/src/db.rs`) — jauh di bawah 200,
aman.

**Tidak ada backup otomatis di tier gratis.** Untuk data penjualan toko, itu
lubang yang sebaiknya kamu tutup sendiri, minimal `pg_dump` berkala ke mesin
lokal.

### 1.4 Project terjeda setelah 7 hari menganggur

Supabase menjeda project gratis setelah **7 hari aktivitas rendah**.
Membangunkannya **harus manual** lewat dashboard — tidak bangun sendiri saat
ada koneksi masuk. Beberapa permintaan database per hari sudah cukup untuk
mencegahnya, dan ping keep-alive di §7 mengurus ini sekaligus.

Kalau sampai terjeda: backend yang restart akan gagal connect dan **langsung
keluar** (`backend/src/main.rs` sengaja mati cepat), lalu Render crash-loop.
Pulihnya: unpause di dashboard Supabase, lalu restart service di Render.

### 1.5 Migrasi jalan sendiri

Backend menjalankan `sqlx::migrate!` sebelum membuka port, jadi versi baru
tidak pernah menerima trafik di atas skema lama. Aman dipanggil berulang —
sqlx mencatat yang sudah jalan di tabel `_sqlx_migrations`.

Dua hal yang perlu dipastikan:

1. **Role harus boleh `CREATE EXTENSION`.** Migrasi `0004` memasang `pg_trgm`
   untuk indeks pencarian produk. Role `postgres` di Supabase bisa.
2. **Rebuild kalau menambah file migrasi.** `sqlx::migrate!` menanam isi
   folder `db/migrations` saat *compile*, bukan saat run. Binary lama yang
   di-restart akan diam-diam melewati migrasi baru tanpa galat. Deploy dari
   Git mengurus ini sendiri karena tiap push memicu build ulang.

Setelah start pertama, periksa:

```sql
select version, description from _sqlx_migrations order by version;
select extname, nspname from pg_extension e
  join pg_namespace n on n.oid = e.extnamespace where extname = 'pg_trgm';
```

Kalau `pg_trgm` terpasang di schema `extensions` (mis. pernah diaktifkan lewat
dashboard), pastikan `extensions` ada di `search_path` role-nya — tanpa itu
`gin_trgm_ops` tidak ketemu dan migrasi indeks gagal.

### 1.6 Buat akun owner pertama

Tidak ada endpoint pendaftaran — akun pertama dibuat langsung di database.
**Jangan pakai `db/seed/dev.sql` atau `scripts/seed-perf.sh` di produksi**;
keduanya berisi password contoh dan data uji.

Password di-hash dengan **bcrypt**:

```bash
# Python (menghasilkan $2b$, sama seperti hash di seed dev)
python -c "import bcrypt,sys; print(bcrypt.hashpw(sys.argv[1].encode(), bcrypt.gensalt(10)).decode())" 'PasswordKuat123'

# atau, kalau apache2-utils tersedia (menghasilkan $2y$)
htpasswd -bnBC 10 "" 'PasswordKuat123' | tr -d ':\n'
```

Lalu di SQL editor Supabase:

```sql
insert into users (name, email_or_username, password_hash, role, is_active)
values ('Owner Toko', 'owner', '<HASH>', 'owner', true);
```

Langsung coba login setelahnya. Kalau ditolak padahal password benar,
hash-nya bervarian yang tidak diterima — buat ulang dengan cara Python.

---

## 2. Variabel lingkungan

### Backend (di Render)

| Variabel | Wajib | Keterangan |
| --- | --- | --- |
| `DATABASE_URL` | ✅ | Session pooler Supabase, role `postgres` (§1.1, §1.2) |
| `JWT_SECRET` | ✅ | Penanda tangan token login. **Ganti dari nilai dev.** Menggantinya membuat semua sesi berjalan tidak berlaku |
| `TOKEN_ENCRYPTION_KEY` | ✅ | 64 karakter hex. Mengenkripsi token marketplace di tabel `platforms` |
| `CORS_ORIGINS` | — | Isi URL Vercel produksi |
| `PORT` | — | **Jangan diisi.** Render menyetelnya sendiri, dan backend sudah membacanya |

```bash
openssl rand -hex 32   # TOKEN_ENCRYPTION_KEY (harus tepat 32 byte)
openssl rand -hex 32   # JWT_SECRET
```

Backend **menolak start** dengan pesan jelas kalau ada yang wajib tapi kosong
atau salah bentuk (`backend/src/config.rs`) — disengaja, supaya kesalahan
setelan ketahuan saat start, bukan di tengah permintaan pertama yang
kebetulan menyentuhnya.

`CORS_ORIGINS` hanya berpengaruh kalau browser memanggil API langsung. Dengan
susunan sekarang itu tidak terjadi, tapi tetap isi dengan URL Vercel — supaya
tetap benar kalau nanti ada halaman yang memanggil API dari sisi klien.

### Web (di Vercel)

| Variabel | Keterangan |
| --- | --- |
| `BACKEND_URL` | URL service Render, mis. `https://aj33-backend.onrender.com`. **Tanpa** `/api` di belakang |

`BACKEND_URL` dibaca saat permintaan berjalan lewat `process.env`, bukan saat
build — jadi satu build yang sama bisa dipakai preview dan produksi.

> Dulu variabel ini dibaca lewat `import.meta.env` saja, yang diganti Vite
> dengan nilai literalnya saat build: alamat mesin pem-build ikut terbawa ke
> hasil build dan menyetelnya di server tidak berpengaruh. Sudah diperbaiki
> di `web/src/lib/api.ts`.

---

## 3. Prasyarat

**Repo harus ada di GitHub.** Render dan Vercel keduanya deploy dari Git. Saat
panduan ini ditulis, repo ini **belum punya satu commit pun** — semua file
masih untracked.

```bash
git add -A
git commit -m "Siap deploy"
git push -u origin main
```

Remote `origin` sudah menunjuk `https://github.com/plutouououo/aj33.git`.
Pastikan repo-nya **private** -- ini berisi logika bisnis toko.

Pastikan ikut ter-commit:

- **`backend/.sqlx/`** (54 file) — cache query sqlx. Tanpa ini build di Render
  gagal karena `SQLX_OFFLINE=true` tidak punya rujukan. Sudah dipastikan tidak
  masuk `.gitignore`.
- **`db/migrations/`** — ditanam ke binary saat compile.

Yang **tidak** boleh ikut: `backend/.env`. Sudah tercakup `.gitignore`.

---

## 4. Deploy backend ke Render

**New → Web Service → connect repo GitHub.**

| Setelan | Nilai |
| --- | --- |
| Root Directory | `backend` |
| Language / Runtime | Rust |
| Build Command | `SQLX_OFFLINE=true cargo build --release` |
| Start Command | `./target/release/bozz-backend` |
| Health Check Path | `/api/health` |
| Instance Type | Free |
| Region | Singapore (samakan dengan Supabase dan Vercel) |

Lalu isi environment variable dari §2.

`SQLX_OFFLINE=true` membuat build memakai cache di `backend/.sqlx/`, jadi
**tidak perlu koneksi database saat build**. CI sudah memverifikasi cache itu
masih sesuai skema (`cargo sqlx prepare --check`); kalau langkah itu merah di
GitHub Actions, jangan deploy — bentuk query di cache sudah berbeda dari
skema.

Build pertama lama (kompilasi Rust dari nol, belasan menit). Build berikutnya
lebih cepat karena cache.

> Kalau runtime **Rust** tidak tersedia di dashboard Render-mu, jalurnya harus
> lewat Docker — dan repo ini belum punya Dockerfile. Lihat §9.

Verifikasi:

```bash
curl https://<service>.onrender.com/api/health   # harus "ok"
```

Endpoint ini ikut menanyai database, jadi jawaban `ok` berarti koneksi,
migrasi, dan skema benar-benar bekerja. Setelah ini, buat akun owner (§1.6).

### Batas tier gratis Render

- **750 jam instance per bulan, per workspace** — bukan per service. Sebulan
  31 hari = 744 jam, jadi **hanya boleh satu service yang hidup terus**.
  Menambah service gratis kedua akan menembus jatah.
- **Tidur setelah 15 menit menganggur**, bangun lagi sekitar **satu menit**.
  Ini yang diatasi §7.
- Tanpa disk permanen, tanpa akses shell, tanpa cron job. Untuk aj33 tidak
  masalah — semua state ada di Supabase.
- Tanpa metode pembayaran terdaftar, Render **menangguhkan** service gratis
  kalau jatah bandwidth terlewati.

*Belum terkonfirmasi dari dokumentasi resmi Render:* besaran bandwidth
sekarang (ada kabar turun jadi 5 GB/bulan sejak April 2026) dan spesifikasi
instance gratis (kabarnya 512 MB / 0.1 CPU). Untuk satu toko, 5 GB masih lega
— trafiknya HTML, bukan media.

---

## 5. Ganti adapter Astro ke Vercel

Proyek ini memakai **Astro 5.16.2**. Versi adapter harus cocok dengan itu.

```bash
cd web
npm uninstall @astrojs/node
npm install @astrojs/vercel@9
```

> **Jangan `npx astro add vercel`.** Perintah itu memasang
> `@astrojs/vercel@11` yang menuntut `astro ^7`, dan build langsung rusak.
> Astro 5 butuh baris `@astrojs/vercel@9`. Kalau nanti Astro dinaikkan,
> adapternya harus ikut naik bersamaan.

`web/astro.config.mjs`:

```js
import { defineConfig } from 'astro/config';
import vercel from '@astrojs/vercel';   // bukan '@astrojs/vercel/serverless' -- subpath itu sudah dihapus
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  output: 'server',
  adapter: vercel(),
  vite: { plugins: [tailwindcss()] },
});
```

`output: 'server'` tetap — halaman ini butuh render per permintaan untuk
membaca cookie sesi.

Setelah ganti adapter, script `npm run preview` (`node ./dist/server/entry.mjs`)
tidak berlaku lagi; untuk mencoba lokal pakai `npm run dev`.

---

## 6. Deploy web ke Vercel

**Add New → Project → import repo.**

| Setelan | Nilai |
| --- | --- |
| Root Directory | `web` |
| Framework Preset | Astro (terdeteksi sendiri) |
| Environment Variable | `BACKEND_URL` = URL Render dari §4 |
| Region | Singapore (`sin1`) |

Setelah dapat URL produksinya, kembali ke Render dan isi `CORS_ORIGINS`
dengan URL itu.

HTTPS sudah otomatis dari Vercel — dan itu wajib, bukan opsional: cookie sesi
dipasang dengan `secure: true` pada build produksi (`web/src/lib/session.ts`).
Di atas HTTP polos browser membuang cookie-nya diam-diam, sehingga login
seperti berhasil lalu halaman berikutnya melempar balik ke `/login` tanpa
pesan apa pun.

---

## 7. Keep-alive: menjaga backend dan database tetap bangun

Tanpa ini, kasir menunggu sekitar satu menit di transaksi pertama setiap kali
toko sepi lebih dari 15 menit — dan database terjeda kalau toko tutup
seminggu.

**Ping `https://<service>.onrender.com/api/health` setiap 10 menit.**

`/api/health` menjalankan `SELECT 1` ke database, jadi satu ping mengurus
keduanya sekaligus: kontainer Render tidak pernah tidur, dan Supabase melihat
aktivitas tiap hari sehingga tidak menjeda project.

Yang meng-ping jangan dari Render sendiri — tier gratisnya tidak punya cron.
Pakai monitor uptime gratis (UptimeRobot, cron-job.org). Ini lebih andal
daripada GitHub Actions terjadwal, yang jadwalnya sering telat dan otomatis
dimatikan kalau repo menganggur 60 hari.

Hitungannya muat: 24 × 31 = 744 jam, di bawah jatah 750 jam/bulan — dengan
sisa 6 jam. Karena itu **jangan menambah service gratis lain di workspace
Render yang sama.**

> **Yang perlu disadari:** menjaga service hidup 24 jam membatalkan maksud
> tier gratis yang dirancang untuk tidur. Saya tidak menemukan konfirmasi
> apakah Render melarangnya — banyak yang melakukannya dan selama ini
> dibiarkan, tapi itu bukan jaminan. Kalau suatu saat celah ini ditutup,
> konsekuensinya kembali ke cold start satu menit.

---

## 8. Urutan deploy pertama

1. Push repo ke GitHub (§3).
2. Buat project Supabase, salin **session pooler** connection string (§1.1).
3. Bangkitkan `JWT_SECRET` dan `TOKEN_ENCRYPTION_KEY` (§2).
4. Deploy backend ke Render (§4). Start pertama menjalankan seluruh migrasi.
5. `curl https://<service>.onrender.com/api/health` → harus `ok`.
6. Buat akun owner di SQL editor Supabase (§1.6).
7. Ganti adapter ke Vercel, commit, push (§5).
8. Deploy web ke Vercel dengan `BACKEND_URL` (§6).
9. Isi `CORS_ORIGINS` di Render dengan URL Vercel.
10. Pasang monitor keep-alive (§7).
11. Buka situsnya, login sebagai owner, buka halaman Produk.

**Deploy berikutnya:** cukup push ke `main`. Render dan Vercel masing-masing
build ulang sendiri. Backend menjalankan migrasi sebelum membuka port, jadi
tidak ada jendela di mana versi baru melayani trafik di atas skema lama.

---

## 9. Kalau bermasalah

| Gejala | Sebab yang paling sering |
| --- | --- |
| Login selalu "Username atau password salah" | Role di `DATABASE_URL` bukan pemilik tabel → RLS mengembalikan nol baris (§1.2) |
| `prepared statement ... already exists`, galat muncul-hilang | Memakai transaction pooler port 6543 (§1.1) |
| Permintaan pertama lama sekali lalu normal | Render baru bangun dari tidur. Pasang keep-alive (§7) |
| Backend crash-loop di Render | Supabase terjeda. Unpause manual, lalu restart service (§1.4) |
| Render: "Konfigurasi tidak lengkap" lalu keluar | Ada env wajib yang kosong. Pesannya menyebut namanya |
| Render: "Migrasi database gagal" | Role tidak boleh `CREATE EXTENSION`, atau skema diubah manual |
| Build Render gagal di query sqlx | `backend/.sqlx/` tidak ikut ter-commit (§3) |
| Login berhasil lalu langsung balik ke `/login` | Domain belum HTTPS (§6) |
| Web menampilkan galat koneksi | `BACKEND_URL` salah, atau ada `/api` di belakangnya |
| Build Vercel gagal soal versi Astro | Adapter `@astrojs/vercel@11` terpasang padahal Astro 5. Turunkan ke `@9` (§5) |
| Halaman lambat padahal backend hangat | Region ketiga penyedia tidak sama |

---

## 10. Yang belum ada

Jujur saja, ini belum lengkap sebagai jalur produksi:

- **Tidak ada rate limit di endpoint login.** Backend sekarang publik, dan
  `POST /api/auth/login` adalah satu-satunya permukaan bisnis tanpa
  autentikasi. Login memang sudah tahan timing attack (verifikasi tetap
  dijalankan terhadap hash umpan saat user tidak ditemukan), tapi tidak ada
  yang menahan percobaan berulang.
- **Tidak ada backup.** Supabase free tidak menyediakannya (§1.3).
- **Backend mati kalau database sedang tidak bisa dihubungi** saat start, alih-alih
  menunggu dan mencoba lagi (§1.4).
- **Tidak ada Dockerfile.** Kalau Render tidak menawarkan runtime Rust,
  jalur ini buntu sampai Dockerfile dibuat.
- **Tidak ada workflow CD.** `.github/workflows/ci.yml` hanya menguji;
  deploy-nya dipicu Render dan Vercel langsung dari push.
- **Tidak ada pemantauan** selain `/api/health` dan monitor keep-alive.
