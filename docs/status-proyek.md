# Status Proyek AJ33

Ringkasan keadaan per **14 September 2026**. Dokumen serah-terima: cukup baca
ini untuk tahu apa yang sudah jadi, apa yang belum, dan jebakan apa saja yang
sudah pernah memakan waktu.

Panduan terpisah: [`panduan-deploy.md`](panduan-deploy.md) ·
[`panduan-ui.md`](panduan-ui.md)

---

## Apa ini

POS multi-platform untuk satu toko: kasir di tempat, katalog produk dengan
stok, pesanan marketplace (TikTok Shop), dan tiket packing untuk pengepak.
Tiga peran: `owner`, `kasir`, `pengepak`.

## Susunan

```
browser ──443──▶ Caddy ──▶ web :4321 (Astro SSR) ──▶ backend :3000 (Rust) ──▶ Postgres
                  (TLS)         semuanya di 127.0.0.1 kecuali Caddy
```

- **backend** — Rust + Axum 0.8 + sqlx 0.8 (Postgres), JWT HS256, bcrypt
- **web** — Astro 5.16 SSR + `@astrojs/node@9` (standalone) + Tailwind v4
- **db** — PostgreSQL 17 di VPS yang sama

Browser **tidak pernah** memanggil backend langsung; semua panggilan API
terjadi di dalam server Astro saat merender. Sesi disimpan di cookie httpOnly
`aj33_sesi`.

Seluruh form memakai POST HTML biasa — tidak ada JavaScript di jalur kritis.
JS hanya dipakai untuk tombol ciutkan sidebar.

## Produksi

| | |
| --- | --- |
| URL | https://tokoayamaj33.my.id (www dialihkan ke sana) |
| VPS | Biznet Gio, `139.190.97.15`, Debian 13, 1 vCPU / 2 GB / 60 GB, Jakarta |
| SSH | `ssh tokoaj33` (alias sudah ada di `~/.ssh/config`) |
| Repo | github.com/plutouououo/aj33 |

**Tata letak di VPS**

```
/opt/aj33/repo         kode + cache build (target/, node_modules/)
/opt/aj33/rilis/<ts>/  hasil build siap pakai, 5 terakhir disimpan
/opt/aj33/aktif        symlink -> rilis yang sedang melayani
/etc/aj33/*.env        konfigurasi + rahasia (640 root:tokoaj33)
/var/backups/aj33/     dump harian
/var/lib/aj33/         status pemantauan
```

**Perintah operasional**

```bash
aj33-deploy     # pull, build, rakit rilis baru, tukar symlink, restart
aj33-rollback   # kembali ke rilis sebelumnya, hitungan detik
aj33-backup     # cadangkan sekarang (di luar jadwal harian 02:10)
aj33-cek        # periksa kesehatan sekarang
```

**Service & timer:** `aj33-backend`, `aj33-web`, `caddy`, `postgresql`,
`aj33-backup.timer` (harian 02:10), `aj33-cek.timer` (tiap 10 menit).

Log: `journalctl -u aj33-backend -f` (ganti unit sesuai kebutuhan).

---

## Yang dikerjakan di sesi ini

**Halaman tiket packing** (`/tiket`, `/tiket/[id]`) — antrean urut tenggat,
checklist barang, alur status ambil → kemas → selesai → serahkan. Stok
berkurang hanya saat serah-terima.

**Perombakan UI** — palet oranye diganti merah bata, topbar diganti sidebar
berkelompok, halaman produk dibangun ulang (saringan, tabel berkepala merah,
paginasi). Ditulis di `panduan-ui.md`.

**Harga per marketplace + lokasi penyimpanan** (migrasi `0005`) — kolom
`price_shopee`, `price_tiktok` (mencakup Tokopedia, satu kanal), dan
`storage_location` di tabel `products`. Lokasi rak tampil di tiket packing dan
**daftar barangnya diurutkan per rak**, supaya pengepak menyusuri gudang sekali
jalan. Lokasi dibaca hidup lewat JOIN, bukan disalin ke `ticket_items` — kalau
barang dipindah rak, salinan lama justru menyesatkan.

**Indeks pencarian produk** (migrasi `0004`) — `pg_trgm` GIN untuk `ILIKE` dan
B-tree untuk `ORDER BY name`. Diukur pada 100 ribu produk: `count(*)`
pencarian 63 ms → 7,8 ms, paginasi dalam 146 ms → 25,6 ms.

**Deploy penuh ke VPS** — dari mesin kosong sampai hidup di internet dengan
TLS otomatis.

**Rate limit login** — 10 percobaan gagal per username per 15 menit.

**Pemantauan** — `aj33-cek` tiap 10 menit.

**Skrip operasional** — backup harian dengan verifikasi, deploy atomik,
rollback.

---

## Jebakan yang sudah memakan waktu

Dicatat supaya tidak terulang.

**1. RLS tanpa policy.** `0002_rls_and_checks.sql` menyalakan RLS di 30 tabel
tanpa satu pun policy. Hanya role pemilik tabel yang bisa membaca. Role yang
salah membuat backend tetap hidup, `/api/health` tetap hijau, dan **semua query
mengembalikan nol baris tanpa error**. Gejalanya: login selalu ditolak padahal
data ada.

**2. `sqlx::migrate!` menanam migrasi saat compile.** Binary lama yang
di-restart melewati migrasi baru tanpa galat apa pun. Pernah terjadi: migrasi
`0004` seolah jalan, indeksnya tidak ada.

**3. `security.allowedDomains` di Astro.** Sejak 5.14, kalau kosong, header
`Host` tidak dipercaya dan hostname jatuh ke `localhost` — **seluruh form POST
ditolak** dengan *"Cross-site POST form submissions are forbidden"*, termasuk
login. Memasang `X-Forwarded-Proto`/`Host` dengan benar tidak menolong, karena
header itu dibuang oleh validasi yang sama.

**4. `import.meta.env` diganti saat build.** `BACKEND_URL` sempat ter-hardcode
jadi `http://localhost:3000` di hasil build. Harus dibaca lewat `process.env`
saat runtime.

**5. `<=` di dalam ekspresi template Astro** dibaca sebagai awal tag Fragment
dan menggagalkan build. Tulis terbalik: `batas >= stok`.

**6. `/usr/sbin` di luar PATH pengguna biasa** di Debian. `adduser`, `swapon`,
`ufw` menjawab `command not found` walaupun terpasang. Ini juga membuat
pemeriksaan salah baca — sempat dilaporkan "swap belum aktif" padahal sudah.

**7. Image Debian ini tidak punya `cron`.** Dipakai systemd timer, yang memang
lebih baik: `Persistent=true` menjalankan cadangan yang terlewat kalau VPS mati
saat jadwalnya tiba.

**8. Tailwind v4: `@apply` hanya menerima utility**, bukan kelas komponen
sendiri. Karena itu `tombol` dan `lencana` dideklarasikan dengan `@utility`.

**9. Kalau suatu saat kembali ke Supabase:** pakai *session pooler* (5432),
jangan *transaction pooler* (6543) — PgBouncer mode transaction tidak mendukung
prepared statement, dan sqlx memakainya untuk tiap query.

---

## Yang belum dikerjakan

**Keamanan & operasional**

- `fail2ban` belum ada. Log sshd menunjukkan pemindaian otomatis terus-menerus.
- Saluran alarm pemantauan belum dipilih. `aj33-cek` siap mengirim ke Telegram
  kalau `/etc/aj33/pantau.env` diisi `TELEGRAM_TOKEN` dan `TELEGRAM_CHAT_ID`;
  tanpa itu hasilnya hanya masuk journal.
- Pengawas dari luar belum ada. `aj33-cek` tidak bisa melapor kalau VPS mati
  total — perlu UptimeRobot atau sejenisnya.
- Backup belum otomatis tersalin ke luar VPS. Manual:
  `rsync -av tokoaj33:/var/backups/aj33/ ~/backup-aj33/`
- **Project Supabase lama masih ada dan tidak terpakai.** Password databasenya
  pernah tercetak di transkrip percakapan — sebaiknya di-reset atau projectnya
  dihapus.

**Fitur**

- Upload gambar produk. Sudah dibahas dan diputuskan arahnya: **simpan di disk
  VPS, bukan S3** (51 GB kosong; 5.000 produk × 3 foto ≈ 4,5 GB). Disajikan
  Caddy langsung, dan direktorinya ikut masuk backup. Kolom
  `products.image_url` sudah mengalir penuh di backend — tinggal endpoint
  upload multipart dan isian file di form. Catatan: tabel `product_images`
  **bukan** untuk ini (terikat `channel_listing_id`, isinya gambar listing
  marketplace).
- Halaman `/pesanan` dan `/pengaturan/platform` ada di menu tapi belum dibuat —
  tautan mati.
- Halaman edit produk belum ada. Kolom Aksi di tabel produk baru berisi
  Aktifkan/Nonaktifkan. Import, Mapping, dan Stok belum punya endpoint.
- Harga marketplace belum ditampilkan di tabel produk (baru ada di form tambah).
- Sortir kolom di tabel produk belum ada; backend selalu `ORDER BY name`.

**Mungkin tidak perlu**

- Retry koneksi database di `backend/src/db.rs`. Dulu dianggap perlu karena
  backend mati kalau database belum siap saat start. `Restart=always` +
  `After=postgresql.service` di systemd sudah menanganinya.

---

## Catatan kerja lokal

- **Docker** dipakai untuk Postgres lokal (`docker-compose.yml`, port 5433).
  Sering mati di mesin ini; kalau perlu database, nyalakan dulu.
- **`backend/.env`** saat ini menunjuk ke **Supabase**, bukan Postgres lokal.
  Menjalankan backend lokal berarti menyentuh database itu, termasuk menjalankan
  migrasi ke sana.
- **`cargo`** terpasang di host (`/d/rust/cargo/bin`), `sqlx-cli` juga.
- **Agent SSH** sering kehilangan kunci setelah laptop mati. Pemulihannya:
  ```bash
  eval "$(ssh-agent -a ~/.ssh/agent.sock)"
  SSH_AUTH_SOCK=~/.ssh/agent.sock ssh-add ~/.ssh/id_ed25519
  ```
  Passphrase harus diketik di terminal sungguhan, bukan lewat prompt Claude Code.
- **Seed**: `scripts/seed-dev.sh` (akun contoh) dan `scripts/seed-perf.sh`
  (produk massal untuk uji performa). **Keduanya hanya untuk development.**

## Perintah verifikasi cepat

```bash
# dari mana saja
curl -s -o /dev/null -w '%{http_code}\n' https://tokoayamaj33.my.id/login

# di VPS
aj33-cek
systemctl is-active aj33-backend aj33-web caddy postgresql
sudo -u postgres psql -d aj33 -tAc "SELECT version, description FROM _sqlx_migrations ORDER BY version"
```
