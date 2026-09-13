# Panduan Deploy AJ33 di VPS

Semua berjalan di satu mesin: 1 vCPU, 2 GB RAM, 60 GB disk, Debian.

```
                        ┌──────────────── VPS ────────────────┐
internet ──443──▶ Caddy │ ──▶ web :4321 (Astro/Node)          │
                  (TLS) │        └──▶ backend :3000 (Rust)    │
                        │                └──▶ Postgres :5432  │
                        └─────────────────────────────────────┘
```

**Hanya Caddy yang menghadap internet.** Web, backend, dan Postgres semuanya
mendengar di `127.0.0.1` saja — tidak bisa dijangkau dari luar, titik. Itu
menghapus seluruh permukaan serangan terhadap API dan database sekaligus.

Perkiraan pemakaian RAM saat jalan: OS ~120 MB, Postgres ~200 MB, Node ~200 MB,
backend Rust ~40 MB, Caddy ~25 MB — sekitar **600 MB dari 2 GB**.

## Prasyarat

- **Nama domain, dan sudah diarahkan ke IP VPS.** Bukan opsional — lihat di
  bawah.
- Repo sudah ada di GitHub, termasuk `backend/.sqlx/` (cache query sqlx) dan
  `db/migrations/`.

### Kenapa domain wajib

Caddy mengambil sertifikat TLS otomatis dari Let's Encrypt, dan Let's Encrypt
hanya menerbitkan sertifikat untuk nama domain — alamat IP tidak bisa.

Dan TLS sendiri tidak bisa dilewati: cookie sesi dipasang `secure: true` pada
build produksi (`web/src/lib/session.ts`). Di atas HTTP polos, browser
menerima cookie itu lalu membuangnya diam-diam — login seolah berhasil, tapi
halaman berikutnya melempar balik ke `/login` tanpa pesan apa pun. Gejalanya
menyesatkan dan bisa menghabiskan waktu berjam-jam kalau tidak tahu sebabnya.

Domain `.my.id` harganya belasan ribu rupiah setahun. Registrarnya bisa
DomaiNesia, Niagahoster, Rumahweb, IDwebhost. Catatan: `.my.id` hanya untuk
warga Indonesia dan pendaftarannya **meminta unggahan KTP** — biasanya
diverifikasi dalam hitungan jam. Kalau ingin langsung aktif tanpa verifikasi,
ambil `.com` atau `.id` biasa.

### Mengarahkan domain ke VPS

**1. Cari IP publik VPS.** Ada di dashboard Biznet Gio, atau tanyakan dari
dalam VPS-nya:

```bash
curl -4 ifconfig.me
```

**2. Buka panel DNS di registrar** tempat domain dibeli — biasanya bernama
"DNS Management", "Kelola DNS", atau "Zone Editor". Tambahkan dua record:

| Tipe | Nama / Host | Nilai | TTL |
| --- | --- | --- | --- |
| A | `@` | `203.0.113.10` (IP VPS-mu) | 3600 |
| A | `www` | `203.0.113.10` | 3600 |

`@` berarti domain telanjang (`tokoku.my.id`); baris kedua membuat
`www.tokoku.my.id` ikut bekerja. Hapus record A atau CNAME bawaan yang
mengarah ke hosting/parking registrar, kalau ada — dua record yang
bertentangan membuat hasilnya berganti-ganti.

**Kalau memakai Cloudflare**, setel awan di sebelah record jadi **abu-abu
(DNS only)**, bukan oranye. Mode proxy oranye membuat Cloudflare yang
menyajikan TLS ke pengunjung, dan sertifikat yang diambil Caddy jadi tidak
pernah terpakai — lebih membingungkan daripada berguna saat menyiapkan ini
pertama kali. Nyalakan proxy-nya belakangan kalau memang mau.

**3. Tunggu sampai benar-benar terselesaikan.** Biasanya beberapa menit.
Periksa dari mesinmu, bukan dari browser (browser menyimpan cache DNS):

```bash
dig +short tokoku.my.id
nslookup tokoku.my.id 8.8.8.8
```

Keduanya harus menjawab dengan IP VPS-mu.

> **Jangan pasang Caddy sebelum perintah di atas menjawab benar.** Let's
> Encrypt membatasi kegagalan validasi — sekitar 5 kali per host per jam.
> Menjalankan Caddy saat DNS belum siap menghabiskan jatah itu, dan kamu
> terkunci satu jam meski DNS-nya sudah benar sesudahnya.

**4. Pastikan port 80 terbuka.** Let's Encrypt memvalidasi kepemilikan domain
dengan menghubungi port 80. Langkah firewall di §1 sudah membukanya bersama
443 — kalau kamu menutup 80 karena merasa semua sudah HTTPS, pengambilan
sertifikat gagal dan perpanjangannya nanti juga gagal.

---

## 1. Siapkan VPS

Aplikasi tidak pernah berjalan sebagai root. Biznet Gio sudah membuatkan
pengguna biasa — di panduan ini `tokoaj33` — yang memegang sudo dan kunci SSH
yang kamu pakai login. Pakai itu; tidak perlu membuat pengguna baru.

Pastikan sudonya memang aktif:

```bash
sudo -v
```

Lalu matikan login password supaya SSH hanya menerima kunci:

```bash
sudo sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo systemctl restart ssh
```

> **Jangan tutup sesi SSH ini** sampai kamu berhasil membuka sesi kedua di
> jendela lain. Kalau setelannya salah dan kamu sudah keluar, satu-satunya
> jalan masuk yang tersisa adalah konsol darurat di dashboard Biznet.

Catatan kalau penyedia hanya memberimu root: buat dulu pengguna biasanya
dengan `adduser`, masukkan ke grup `sudo`, salin kunci SSH ke
`/home/<nama>/.ssh`, lalu lanjutkan sebagai dia. `adduser` ada di
`/usr/sbin`, yang tidak masuk PATH pengguna biasa — panggil lewat `sudo
adduser` atau sebut jalur lengkapnya.

### Swap

Menjalankan aplikasi ini muat di 2 GB, tapi tahap *linking* saat compile Rust
bisa melonjak mendekati batas itu sendirian. Tanpa swap, build mati di tengah
dengan pesan yang tidak menyebut memori sama sekali.

```bash
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### Zona waktu dan firewall

```bash
sudo timedatectl set-timezone Asia/Jakarta

sudo apt update && sudo apt install -y ufw
sudo ufw allow OpenSSH
sudo ufw allow 80,443/tcp
sudo ufw enable
```

Postgres **tidak** dibuka. Bawaan Debian sudah membuatnya mendengar di
localhost saja, dan memang di situlah tempatnya.

---

## 2. Pasang komponen

```bash
sudo apt install -y build-essential pkg-config git curl postgresql

# Rust lewat rustup, bukan apt -- versi apt sering tertinggal jauh.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"

# Node 22 lewat NodeSource.
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt install -y nodejs

# Caddy lewat repo resminya.
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install -y caddy
```

Cek versinya: `rustc --version`, `node --version`, `psql --version`,
`caddy version`.

---

## 3. Database

> **Bagian paling mudah salah di seluruh panduan ini.**

`db/migrations/0002_rls_and_checks.sql` menyalakan Row Level Security di 30
tabel **tanpa satu pun policy**. Hanya role pemilik tabel yang bisa membaca
isinya. Kalau migrasi dijalankan oleh satu role tapi aplikasi menyambung
dengan role lain, backend tetap hidup, `/api/health` tetap hijau, dan **semua
query mengembalikan nol baris tanpa pesan error apa pun**. Gejalanya: login
selalu ditolak "Username atau password salah" padahal datanya jelas ada.

Jadi: satu role, dipakai untuk segalanya. Ia memiliki database dan tabelnya,
sehingga RLS tidak menghalanginya.

```bash
sudo -u postgres psql <<'SQL'
CREATE ROLE aj33 LOGIN PASSWORD 'GANTI_DENGAN_PASSWORD_ACAK';
CREATE DATABASE aj33 OWNER aj33;
SQL
```

Bangkitkan passwordnya dengan `openssl rand -base64 24`, jangan dikarang.
Database ini hanya bisa dihubungi dari dalam VPS, tapi password lemah tetap
tidak ada gunanya.

`DATABASE_URL` yang dipakai nanti:

```
postgresql://aj33:PASSWORD@localhost:5432/aj33
```

### Ekstensi pencarian

Migrasi `0004` memasang `pg_trgm` untuk indeks pencarian produk. Sejak
PostgreSQL 13 ekstensi ini berstatus *trusted*, jadi pemilik database boleh
memasangnya sendiri dan migrasi akan berjalan mulus. Kalau ternyata ditolak,
pasang sekali sebagai superuser lalu ulangi:

```bash
sudo -u postgres psql -d aj33 -c 'CREATE EXTENSION IF NOT EXISTS pg_trgm;'
```

---

## 4. Ambil kode dan siapkan konfigurasi

Tata letaknya memisahkan *tempat membangun* dari *yang sedang berjalan*:

```
/opt/aj33/
├── repo/          kode + cache build (target/, node_modules/)
├── rilis/
│   ├── 20260912-2140/   hasil build, siap pakai
│   └── 20260912-2015/   rilis sebelumnya, disimpan untuk rollback
└── aktif -> rilis/20260912-2140
```

Service systemd menunjuk ke `aktif`, tidak pernah ke `repo`. Karena itu
membangun versi baru tidak pernah menyentuh yang sedang melayani pelanggan,
dan kembali ke versi lama cukup memindahkan satu symlink -- hitungan detik,
bukan compile ulang 30 menit.

```bash
sudo mkdir -p /opt/aj33/rilis && sudo chown -R tokoaj33:tokoaj33 /opt/aj33
git clone https://github.com/plutouououo/aj33.git /opt/aj33/repo
```

Konfigurasi ditaruh di `/etc/aj33/`, terpisah dari kode — supaya `git pull`
tidak pernah bisa menimpanya, dan supaya rahasianya tidak ikut ter-commit.

**1. Bangkitkan rahasianya lebih dulu**, karena nilainya dipakai di langkah
berikutnya. `TOKEN_ENCRYPTION_KEY` wajib tepat 32 byte, jadi jangan dikarang:

```bash
sudo mkdir -p /etc/aj33
openssl rand -hex 32   # untuk TOKEN_ENCRYPTION_KEY
openssl rand -hex 32   # untuk JWT_SECRET
```

Salin kedua keluarannya; keduanya berbeda dan jangan tertukar.

**2. Buat `/etc/aj33/backend.env`.** Ganti `PASSWORD` dengan password role
Postgres dari §3, dan kedua rahasia dengan hasil `openssl` di atas:

```bash
sudo tee /etc/aj33/backend.env >/dev/null <<'EOF'
DATABASE_URL=postgresql://aj33:PASSWORD@localhost:5432/aj33
JWT_SECRET=GANTI_DENGAN_HASIL_OPENSSL_KEDUA
TOKEN_ENCRYPTION_KEY=GANTI_DENGAN_HASIL_OPENSSL_PERTAMA
PORT=3000
CORS_ORIGINS=https://tokoku.my.id
EOF
```

(`<<'EOF'` dengan tanda kutip membuat isinya ditulis apa adanya — tanpa itu,
shell akan mencoba menafsirkan `$` di dalam password sebagai variabel.)

**3. Buat `/etc/aj33/web.env`.** Yang ini tidak berisi rahasia apa pun, jadi
bisa disalin utuh:

```bash
sudo tee /etc/aj33/web.env >/dev/null <<'EOF'
HOST=127.0.0.1
PORT=4321
BACKEND_URL=http://127.0.0.1:3000
EOF
```

**4. Kunci izinnya**, karena `backend.env` berisi kredensial database:

```bash
sudo chown -R root:tokoaj33 /etc/aj33
sudo chmod 750 /etc/aj33
sudo chmod 640 /etc/aj33/*.env
```

Periksa hasilnya — kalau `chmod` mengeluh `No such file or directory`,
berarti kedua file di atas belum benar-benar terbuat:

```bash
ls -l /etc/aj33
```

Yang diharapkan: dua file `-rw-r-----` milik `root:tokoaj33`. Root menulis,
`tokoaj33` (yang menjalankan service) hanya membaca, pengguna lain tidak
kebagian apa-apa.

Backend **menolak start** dengan pesan jelas kalau ada yang wajib tapi kosong
atau salah bentuk (`backend/src/config.rs`) — disengaja, supaya kesalahan
setelan ketahuan saat start, bukan di tengah permintaan pertama yang
kebetulan menyentuhnya.

`CORS_ORIGINS` praktis tidak terpakai di susunan ini: browser tidak pernah
memanggil backend langsung, semua panggilan API terjadi di dalam server Astro
saat merender. Tetap diisi benar supaya tidak menyimpan bom waktu kalau nanti
ada halaman yang memanggil API dari sisi klien.

---

## 5. Build

Build pertama dijalankan tangan; selanjutnya skrip `aj33-deploy` (§10) yang
mengerjakannya.

```bash
cd /opt/aj33/repo/backend
SQLX_OFFLINE=true cargo build --release

cd /opt/aj33/repo/web
npm ci
npm run build
```

Lalu rakit rilis pertama dan arahkan `aktif` ke sana:

```bash
AKAR=/opt/aj33; BARU=$AKAR/rilis/$(date +%Y%m%d-%H%M%S)
mkdir -p "$BARU/web"
cp "$AKAR/repo/backend/target/release/aj33-backend" "$BARU/aj33-backend"
cp -r "$AKAR/repo/web/dist" "$BARU/web/dist"
cp "$AKAR/repo/web/package.json" "$AKAR/repo/web/package-lock.json" "$BARU/web/"
cp -al "$AKAR/repo/web/node_modules" "$BARU/web/node_modules"
ln -sfn "$BARU" "$AKAR/aktif"
```

`SQLX_OFFLINE=true` membuat build memakai cache di `backend/.sqlx/` alih-alih
menanyai database saat compile. CI sudah memverifikasi cache itu masih sesuai
skema (`cargo sqlx prepare --check`); kalau langkah itu merah, jangan deploy.

Build Rust pertama mengompilasi sekitar 400 crate di 1 vCPU — **perkirakan
20–40 menit**. Build berikutnya jauh lebih cepat karena hanya kode kita yang
berubah. Kalau tetap kehabisan memori meski sudah ada swap, batasi
paralelismenya: `CARGO_BUILD_JOBS=1 cargo build --release`.

---

## 6. Service systemd

`/etc/systemd/system/aj33-backend.service`:

```ini
[Unit]
Description=AJ33 backend (Rust/Axum)
# Postgres harus siap lebih dulu: backend menjalankan migrasi dan langsung
# keluar kalau database tidak bisa dihubungi.
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
User=tokoaj33
WorkingDirectory=/opt/aj33/aktif
EnvironmentFile=/etc/aj33/backend.env
# Menunjuk symlink, bukan direktori rilis. systemd menyelesaikan symlink saat
# start, jadi `restart` sesudah symlink dipindah akan menjalankan versi baru.
ExecStart=/opt/aj33/aktif/aj33-backend
# Backend sengaja mati cepat kalau database belum siap. Restart otomatis
# inilah yang membuat sikap itu aman: systemd terus mencoba sampai Postgres
# bangun, alih-alih meninggalkan aplikasi dalam keadaan mati.
Restart=always
RestartSec=5

# Pengetatan: proses ini tidak pernah perlu menulis ke disk mana pun.
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true

[Install]
WantedBy=multi-user.target
```

`/etc/systemd/system/aj33-web.service`:

```ini
[Unit]
Description=AJ33 web (Astro SSR)
After=network-online.target aj33-backend.service
Wants=network-online.target

[Service]
User=tokoaj33
WorkingDirectory=/opt/aj33/aktif/web
EnvironmentFile=/etc/aj33/web.env
ExecStart=/usr/bin/node ./dist/server/entry.mjs
Restart=always
RestartSec=5

NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now aj33-backend aj33-web
sudo systemctl status aj33-backend aj33-web
```

Start pertama backend menjalankan seluruh migrasi sebelum membuka port, jadi
tidak ada jendela di mana versi baru melayani trafik di atas skema lama.

```bash
curl http://127.0.0.1:3000/api/health   # harus "ok"
```

`/api/health` ikut menanyai database, jadi jawaban `ok` berarti koneksi,
migrasi, dan skema benar-benar bekerja.

---

## 7. Caddy

`/etc/caddy/Caddyfile`:

```
tokoku.my.id {
    encode zstd gzip
    reverse_proxy 127.0.0.1:4321
}

# Satu alamat saja yang jadi alamat sungguhan; www dialihkan ke sana. Kalau
# keduanya sama-sama melayani, sesi yang dibuat di satu alamat tidak terbawa
# ke alamat satunya -- cookie terikat pada host, jadi pengguna terlihat
# logout begitu berpindah antara www dan tanpa www.
www.tokoku.my.id {
    redir https://tokoku.my.id{uri} permanent
}
```

```bash
sudo systemctl reload caddy
```

Itu saja. Caddy mengurus sertifikat Let's Encrypt, perpanjangannya, dan
pengalihan HTTP ke HTTPS sendiri — asalkan A record domain sudah menunjuk ke
IP VPS sebelum perintah di atas dijalankan.

Perhatikan Caddy hanya mengenal port **4321**. Backend tidak pernah disebut,
karena memang tidak boleh dijangkau dari luar.

---

## 8. Akun owner pertama

Tidak ada endpoint pendaftaran — akun pertama dibuat langsung di database.
**Jangan menjalankan `db/seed/dev.sql` atau `scripts/seed-perf.sh` di sini**;
keduanya berisi password contoh dan data uji.

Password di-hash dengan bcrypt:

```bash
sudo apt install -y apache2-utils
htpasswd -bnBC 10 "" 'PasswordKuat123' | tr -d ':\n'
```

```bash
sudo -u postgres psql -d aj33 <<SQL
INSERT INTO users (name, email_or_username, password_hash, role, is_active)
VALUES ('Owner Toko', 'owner', '<HASH>', 'owner', true);
SQL
```

Langsung coba login. Kalau ditolak padahal password benar, hash-nya bervarian
yang tidak diterima — buat ulang dengan Python:

```bash
python3 -c "import bcrypt,sys; print(bcrypt.hashpw(sys.argv[1].encode(), bcrypt.gensalt(10)).decode())" 'PasswordKuat123'
```

---

## 9. Backup — wajib, bukan opsional

Data penjualan toko kini hanya ada di satu mesin. Kalau VPS ini hilang, semua
hilang: transaksi, stok, riwayat. Tidak ada penyedia yang akan mengembalikannya
untukmu.

`/usr/local/bin/aj33-backup`:

```bash
#!/usr/bin/env bash
set -euo pipefail
TUJUAN=/var/backups/aj33
mkdir -p "$TUJUAN"
sudo -u postgres pg_dump -Fc aj33 > "$TUJUAN/aj33-$(date +%F-%H%M).dump"
# Simpan 14 hari terakhir. Lebih lama dari itu tidak menambah perlindungan
# terhadap kerusakan yang baru disadari, dan disk bukan tempat arsip.
find "$TUJUAN" -name 'aj33-*.dump' -mtime +14 -delete
```

```bash
sudo chmod +x /usr/local/bin/aj33-backup
sudo crontab -e
# tiap hari 02:10
10 2 * * * /usr/local/bin/aj33-backup
```

**Dump di VPS yang sama bukan backup.** Ia melindungi dari salah hapus, bukan
dari VPS yang mati. Salin ke luar — paling sederhana, tarik dari laptopmu
secara berkala:

```bash
rsync -av tokoaj33@tokoku.my.id:/var/backups/aj33/ ~/backup-aj33/
```

Dan sesekali **coba pulihkan** ke database kosong. Backup yang tidak pernah
diuji bukan backup, hanya file:

```bash
createdb -U postgres aj33_uji
pg_restore -U postgres -d aj33_uji /var/backups/aj33/aj33-XXXX.dump
```

---

## 10. Deploy ulang dan rollback

Keduanya bekerja pada tata letak rilis di §4: build dirakit di samping, lalu
symlink `aktif` dipindah dalam satu langkah.

`/usr/local/bin/aj33-deploy`:

```bash
#!/usr/bin/env bash
set -euo pipefail

AKAR=/opt/aj33
BARU="$AKAR/rilis/$(date +%Y%m%d-%H%M%S)"

# Build tetap dilakukan di dalam repo supaya cache cargo dan node_modules
# terpakai. Yang sedang melayani pelanggan ada di direktori lain, jadi tidak
# tersentuh sama sekali selama 30 menit compile.
cd "$AKAR/repo"
git pull --ff-only
( cd backend && SQLX_OFFLINE=true cargo build --release )
( cd web && npm ci && npm run build )

# Rakit rilis baru. node_modules disalin sebagai hardlink: hampir tanpa biaya
# disk dan seketika, tapi rilis ini tetap memegang salinannya sendiri kalau
# `npm ci` berikutnya mengganti isinya -- itulah yang membuat rollback tetap
# utuh berbulan-bulan kemudian.
mkdir -p "$BARU/web"
cp "$AKAR/repo/backend/target/release/aj33-backend" "$BARU/aj33-backend"
cp -r "$AKAR/repo/web/dist" "$BARU/web/dist"
cp "$AKAR/repo/web/package.json" "$AKAR/repo/web/package-lock.json" "$BARU/web/"
cp -al "$AKAR/repo/web/node_modules" "$BARU/web/node_modules"

# Pergantian atomik: `mv -T` menimpa symlink dalam satu operasi kernel. Tidak
# ada saat di mana `aktif` menunjuk ke tempat yang belum lengkap, dan tidak
# ada saat di mana ia tidak menunjuk ke mana-mana.
ln -sfn "$BARU" "$AKAR/aktif.baru"
mv -T "$AKAR/aktif.baru" "$AKAR/aktif"

sudo systemctl restart aj33-backend aj33-web

# Simpan 5 rilis terakhir. Lebih dari itu tidak menambah perlindungan, dan
# tiap rilis memegang salinan dist-nya sendiri.
ls -1 "$AKAR/rilis" | sort -r | tail -n +6   | while read -r lama; do rm -rf "${AKAR:?}/rilis/$lama"; done

echo "Aktif: $(basename "$BARU")"
```

`/usr/local/bin/aj33-rollback`:

```bash
#!/usr/bin/env bash
set -euo pipefail

AKAR=/opt/aj33
SEKARANG=$(basename "$(readlink -f "$AKAR/aktif")")
SEBELUM=$(ls -1 "$AKAR/rilis" | sort -r | grep -v "^${SEKARANG}$" | head -1)

if [[ -z "$SEBELUM" ]]; then
  echo "Tidak ada rilis lain untuk dituju." >&2
  exit 1
fi

ln -sfn "$AKAR/rilis/$SEBELUM" "$AKAR/aktif.baru"
mv -T "$AKAR/aktif.baru" "$AKAR/aktif"
sudo systemctl restart aj33-backend aj33-web

echo "Kembali dari $SEKARANG ke $SEBELUM"
```

```bash
sudo chmod +x /usr/local/bin/aj33-deploy /usr/local/bin/aj33-rollback
```

Deploy yang rusak dibatalkan dengan satu perintah, dalam hitungan detik:

```bash
aj33-rollback
```

### Yang TIDAK ikut mundur saat rollback

**Migrasi database.** Backend menjalankan migrasi saat start dan tidak pernah
membatalkannya. Rollback mengembalikan kode, bukan skema.

Selama migrasinya aditif — menambah kolom atau tabel, seperti `0005` — kode
lama tetap berjalan di atas skema baru tanpa masalah, karena ia hanya
mengabaikan yang tidak dikenalnya. Yang berbahaya adalah migrasi yang
menghapus atau mengganti nama kolom: setelah itu rollback kode akan menabrak
kolom yang sudah tidak ada.

Kalau suatu saat perlu migrasi seperti itu, pecah jadi dua rilis — rilis
pertama berhenti memakai kolomnya, rilis berikutnya baru menghapusnya. Di
antara keduanya rollback tetap aman.

---

## 11. Kalau bermasalah

| Gejala | Sebab yang paling sering |
| --- | --- |
| Login selalu "Username atau password salah" | Role penyambung bukan pemilik tabel → RLS mengembalikan nol baris (§3) |
| Login berhasil lalu langsung balik ke `/login` | Belum HTTPS, cookie `secure` dibuang browser (Prasyarat) |
| Caddy gagal ambil sertifikat | A record belum menunjuk ke IP VPS, atau port 80 tertutup firewall |
| `systemctl status aj33-backend` → "Konfigurasi tidak lengkap" | Ada env wajib yang kosong; pesannya menyebut namanya |
| Backend restart terus-menerus | Postgres belum jalan, atau `DATABASE_URL` salah. `journalctl -u aj33-backend -n 50` |
| Build Rust mati tanpa pesan jelas | Kehabisan memori. Pastikan swap aktif (`free -h`), atau `CARGO_BUILD_JOBS=1` |
| Build gagal di query sqlx | `backend/.sqlx/` tidak ikut ter-commit |
| Web menampilkan galat koneksi | `BACKEND_URL` salah, atau ada `/api` di belakangnya |
| Halaman 502 dari Caddy | Service web mati. `systemctl status aj33-web` |

Log: `journalctl -u aj33-backend -f`, `journalctl -u aj33-web -f`,
`journalctl -u caddy -f`.

---

## 12. Yang belum ada

- **Tidak ada rate limit di `POST /api/auth/login`.** Itu satu-satunya
  permukaan bisnis tanpa autentikasi yang bisa dijangkau dari internet. Login
  sudah tahan timing attack (verifikasi tetap dijalankan terhadap hash umpan
  saat user tidak ditemukan), tapi tidak ada yang menahan percobaan berulang.
  Caddy bisa membatasinya di depan.
- **Tidak ada pemantauan** selain `/api/health` — tidak ada yang memberi tahu
  kalau service mati di luar jam kerja.
- **Tidak ada staging.** Deploy langsung ke satu-satunya mesin yang ada.
- **Backup belum otomatis tersalin ke luar VPS** (§9) — bagian itu masih
  manual dan bergantung pada kedisiplinanmu.
