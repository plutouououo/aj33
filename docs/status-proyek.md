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

Seluruh form memakai POST HTML biasa. JavaScript dipakai untuk tombol ciutkan
sidebar dan — sebagai satu-satunya pengecualian yang disetujui — untuk
mempercepat halaman kasir, yang **tetap berfungsi penuh tanpa JS**.

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

**Deploy: cukup `git push` ke `main`.** GitHub Actions membangun lalu
mengirimnya ke VPS. Lihat `panduan-deploy.md` §10.

**Perintah operasional** (di VPS)

```bash
aj33-rollback   # kembali ke rilis sebelumnya, hitungan detik
aj33-backup     # cadangkan sekarang (di luar jadwal harian 02:10)
aj33-cek        # periksa kesehatan sekarang
aj33-deploy     # build DI VPS -- jalur cadangan kalau Actions bermasalah;
                # memakan CPU penuh 12 menit, jangan saat jam ramai
aj33-terima     # dipanggil CI lewat SSH, bukan oleh manusia
```

**Service & timer:** `aj33-backend`, `aj33-web`, `caddy`, `postgresql`,
`aj33-backup.timer` (harian 02:10), `aj33-cek.timer` (tiap 10 menit).

Log: `journalctl -u aj33-backend -f` (ganti unit sesuai kebutuhan).

---

## Yang dikerjakan di sesi ini

**Diskon dan kanal harga di kasir** (migrasi `0015`) — dua kolom baru di
`transactions`. `discount_amount` menutup pintu yang sengaja dibiarkan tertutup
sejak migrasi 0007 ("diskon punya aturannya sendiri dan belum ada di sistem
ini"): potongan punya kolom sendiri, **tidak dikurangkan dari `subtotal`**,
supaya baris struk tetap bisa dijumlahkan menjadi subtotal. Sejak migrasi ini
omzet barang adalah `SUM(subtotal - discount_amount)` — dasbor, laporan, dan
riwayat pelanggan sudah ikut. `sales_channel` (`toko` / `shopee` / `tiktok`)
memilih daftar harga yang dipakai: selama Shopee dan Tokopedia belum
tersambung, pesanannya dicatat manual di kasir, dan tanpa kanal semuanya
tercatat seharga toko — selisihnya muncul sebagai laba yang tidak pernah ada.
`products.price_shopee` dan `price_tiktok` sudah ada sejak migrasi 0005 tapi
belum pernah punya jalan untuk dipakai saat menjual; sekarang punya. Harga
kanal yang kosong jatuh ke harga dasar, bukan ke nol. Keduanya **wajib ikut
sidik jari `Idempotency-Key`** dan karena itu harus tersimpan: tanpa itu,
keranjang yang dikirim ulang setelah diskon atau kanalnya dibetulkan dijawab
dengan transaksi lama yang nilainya salah, tanpa galat apa pun.

**Lokasi penyimpanan pindah ke batch** (migrasi `0016`) —
`products.storage_location` (migrasi 0005) di-drop, diganti
`product_batches.storage_location`. Satu label per produk benar hanya selama
satu produk tinggal di satu tempat; sejak stok hidup per batch (migrasi 0011),
kiriman yang datang di hari berbeda memang ditaruh di rak berbeda, dan label
yang kadang benar kadang salah lebih buruk daripada tidak ada label — ia tetap
dipercaya. Lokasi kini sejajar dengan kedaluwarsa dan harga beli: ketiganya
sifat kiriman. Tiket packing membaca rak dari batch yang **masih bersisa**,
digabung tanpa pengulangan dan urut FEFO, jadi pengepak tidak dikirim ke rak
yang sudah kosong. Tidak ada "lokasi default" lagi, karena tidak ada tempat
yang bisa dijanjikannya dengan benar.

**Kedaluwarsa dan lokasi batch bisa dikoreksi** — `PATCH
/products/{id}/batches/{batch_id}` yang dulu hanya menerima `purchase_price`
kini menerima ketiganya dengan semantik `Ubah` (tidak disebut = biarkan,
`null` = kosongkan). Di halaman produk ketiganya jadi satu baris tabel dengan
satu tombol Simpan — bukan tiga form sebaris, yang membuat orang menyimpan
satu koreksi lalu mengira dua lainnya ikut tersimpan. **Jumlah batch tetap
tidak bisa disunting**: ia menggerakkan stok, dan stok hanya berubah lewat
ledger. Batch yang salah jumlahnya dibatalkan lalu dicatat ulang.

**Halaman tiket packing** (`/tiket`, `/tiket/[id]`) — antrean urut tenggat,
checklist barang, alur status ambil → kemas → selesai → serahkan. Stok
berkurang hanya saat serah-terima.

**Perombakan UI** — palet oranye diganti merah bata, topbar diganti sidebar
berkelompok, halaman produk dibangun ulang (saringan, tabel berkepala merah,
paginasi). Ditulis di `panduan-ui.md`.

**Harga per marketplace + lokasi penyimpanan** (migrasi `0005`) — kolom
`price_shopee`, `price_tiktok` (mencakup Tokopedia, satu kanal), dan
`storage_location` di tabel `products`. (Lokasi sudah pindah ke
`product_batches` pada migrasi `0016`; sisa paragraf ini keadaan saat itu.)
Lokasi rak tampil di tiket packing dan
**daftar barangnya diurutkan per rak**, supaya pengepak menyusuri gudang sekali
jalan. Lokasi dibaca hidup lewat JOIN, bukan disalin ke `ticket_items` — kalau
barang dipindah rak, salinan lama justru menyesatkan.

**Nama SEO, SKU otomatis, varian, dan batch** (migrasi `0006`) — `products`
dapat `seo_name` (judul untuk marketplace; `name` tinggal jadi nama
identifikasi internal), `product_type`, `variant_color`, `variant_size`, dan
`parent_id`. SKU tidak lagi diketik: dirakit dari **Merek - Jenis Produk -
Warna - Ukuran** dan dirakit ulang setiap atribut itu berubah, dengan akhiran
urut kalau bentrok. Varian adalah baris `products` yang menunjuk induknya,
jadi ikut memakai stok, ledger, kasir, dan tiket yang sudah ada — tiap varian
punya harga sendiri, termasuk harga per marketplace. Barang masuk dicatat
sebagai batch berikut tanggal kedaluwarsanya dan menambah stok lewat ledger
(alasan `restock`), jadi tidak ada butir stok tanpa asal-usul. CRUD produk
lengkap: `GET/POST/PATCH/DELETE /products` plus halaman `/produk/[id]` untuk
menyunting, mengelola varian dan batch, dan mengoreksi stok.

**Katalog disesuaikan dengan barang yang benar-benar dijual** (migrasi `0008`)
— kolom `variant_color` jadi `variant_grade`: sumbu varian toko ayam adalah
mutu dan kelas ukuran ("SP 08", "Super Besar", "B"), bukan warna. Ukuran kini
berarti **isi satu pack**, dan `products.unit` berhenti dipakai — satu SKU
berarti satu pack, satu pack belum tentu satu kilogram, dan stok dihitung per
pack. Kolom `unit` tidak di-drop supaya data lama tidak hilang. Merek dipilih
dari daftar tetap (AFCO, BEST CHICKEN, OK CHICK) di formulir; backend tetap
menerima teks bebas karena impor marketplace bisa membawa merek lain.
Sembilan kategori dipasang: Ayam Utuh, Parting, Dada, Paha, Ceker, Kulit,
Jeroan, Tulang, MDM.

**Format SKU mengikuti kode yang sudah dipakai toko** — `[inisial Jenis +
Grade]-[3 huruf Merek]-[Ukuran]`. Ini bukan aturan baru: toko sudah
memakainya bertahun-tahun, diketik tangan di nama produk. `CBSB` adalah
Ceker Bersih Super Besar, `HJA` adalah Hati Jantung Ampela — dan perakit
otomatisnya menghasilkan kode yang **sama persis**, jadi hafalan pegawai dan
pencarian dengan kode lama tetap bekerja. Ada ujinya. Bagian yang mengandung
angka dibawa utuh ("SP 08" jadi `SP08`, bukan `S0`) supaya kelas ukurannya
tetap terbaca. Merek dipendekkan menurut jumlah katanya: afco → AFC, best
chicken → BEC, OK CHICK → OKC.

**Tanggal kedaluwarsa keluar dari nama produk.** Angka di ekor nama lama
("Ayam Utuh_SP 08 afco 25.7") ternyata tanggal kedaluwarsa, bukan harga —
cara bertahan hidup sebelum ada batch. Akibatnya satu barang muncul berkali
sebagai produk berbeda hanya karena beda kedaluwarsa. Sekarang itu satu
produk dengan beberapa batch.

**Alur tambah produk bergaya Shopee** (`/produk/baru`) — form tambah pindah
ke halaman tersendiri: remah navigasi, bilah tab bagian yang menempel
(Informasi Produk · Atribut & SKU · Informasi Penjualan · Stok & Penyimpanan),
kartu per bagian, dan bilah simpan lengket di dasar layar. **Seluruh isian
disusun ke bawah**, satu per baris — grid menyamping hanya tersisa di bilah
saringan. Tabnya tautan lompat murni HTML; alasan kenapa sengaja tanpa penanda
"tab aktif" ada di `panduan-ui.md`. Simpan yang berhasil memakai
POST/redirect/GET ke halaman produk yang baru dibuat, dan galat validasi tidak
lagi menghapus isian yang sudah diketik. `/produk/[id]` mengikuti pola yang
sama.

**Kasir satu layar** (`/kasir`) — katalog kartu di kiri, panel keranjang dan
pembayaran menempel di kanan, struk muncul di panel yang sama begitu penjualan
berhasil. Menggantikan alur tiga langkah: tiap perpindahan langkah dulu berarti
satu muat ulang untuk mengetahui total, padahal angka itulah yang ditunggu
pembeli di depan meja. Pencarian, filter kategori berbentuk pil, detail per
barang (ukuran, grade, merek, lokasi rak, sisa tiap batch, peringatan
kedaluwarsa ≤30 hari), dan pemilih batch per produk. **Progressive
enhancement**, satu-satunya pengecualian yang disetujui atas prinsip "HTML
dulu": tanpa JavaScript halaman tetap berfungsi penuh (isi jumlah, "Perbarui
keranjang", lalu "Selesaikan"), dengan JavaScript panel keranjang dan seluruh
angkanya berubah seketika. Skripnya hanya menulis ke isian yang sudah ada, jadi
server menerima data yang sama persis di kedua keadaan. Keranjang hidup di
isian form yang sama dari awal sampai bayar — tidak ada lagi salinan hidden
field antar-langkah. Kunci idempotensi dibuat saat halaman dibuka, dibaca ulang
tiap POST, dan hanya diganti setelah penjualan berhasil.

**Pelanggan dan ongkos kirim** (migrasi `0007`) — endpoint `/customers` baru
(daftar + buat; nomor telepon yang sudah ada tidak digandakan), dan kolom
`transactions.shipping_cost`. Ongkir punya kolom sendiri, **bukan dilebur ke
`subtotal`**: subtotal adalah harga barang dan itulah dasar perhitungan
margin. Sejak migrasi ini `SUM(total_amount)` bukan lagi omzet barang, dan
sejak migrasi `0015` omzet barang bukan lagi `SUM(subtotal)` melainkan
`SUM(subtotal - discount_amount)`.

**Bug idempotensi yang sudah lama ada, diperbaiki** — `sidik_jari` mem-hash
nominal lewat `to_string()`, sedangkan `70000` dari request dan `70000.00`
yang dibaca balik dari `NUMERIC(14,2)` menghasilkan teks berbeda. Akibatnya
pengiriman ulang yang sah untuk pembayaran tunai bulat dijawab 409 — justru
kegagalan yang `Idempotency-Key` ada untuk mencegahnya. Uji lama tidak
menangkapnya karena `Decimal` menganggap keduanya sama. Sekarang nominal
selalu diseragamkan ke dua desimal sebelum di-hash, dengan uji regresinya.
Ongkir ikut masuk sidik jari dengan alasan yang sama.

**Indeks pencarian produk** (migrasi `0004`) — `pg_trgm` GIN untuk `ILIKE` dan
B-tree untuk `ORDER BY name`. Diukur pada 100 ribu produk: `count(*)`
pencarian 63 ms → 7,8 ms, paginasi dalam 146 ms → 25,6 ms.

**Deploy penuh ke VPS** — dari mesin kosong sampai hidup di internet dengan
TLS otomatis.

**Rate limit login** — 10 percobaan gagal per username per 15 menit.

**Pemantauan** — `aj33-cek` tiap 10 menit.

**Skrip operasional** — backup harian dengan verifikasi, deploy atomik,
rollback.

**CD lewat GitHub Actions** (`.github/workflows/deploy.yml` +
`scripts/aj33-terima`) — dipicu setelah CI hijau, membangun di runner, lalu
mengirim tarball lewat SSH. Kunci CI dikunci `command=` di `authorized_keys`
sehingga tidak bisa membuka shell; paket dikirim lewat stdin karena forced
command mematikan scp. Penerima memeriksa kesehatan sampai 40 detik dan
**mengembalikan rilis lama sendiri kalau gagal**. Terbukti pada deploy pertama:
rilis mendarat 7,5 menit setelah push, dan binary dari Ubuntu 24.04 berjalan di
Debian 13 dengan jarak glibc yang lega (butuh 2.34, tersedia 2.41).

### Harga ecer dan grosir (migrasi 0022)

- `products.variant_size` kini `NUMERIC(8,3)` berisi **kg per pack** (boleh pecahan). Teks lama dikonversi angkanya ("2 kg" jadi 2, "500 gr" jadi 0,5); yang tak berangka (mis. "Besar") menjadi NULL. SKU lama tidak berubah.
- `products.price` kini berarti **harga ecer**; `products.price_wholesale` (nullable) adalah harga grosir. Penjualan tetap per pack.
- Aturan di `pos/service.rs::SalesChannel::harga`: kanal **toko** memakai grosir bila `qty` pack per produk (batch digabung) **lebih dari** `pricing_settings.wholesale_threshold_qty`; tepat di batas masih ecer. Grosir kosong berarti ecer. Sejak migrasi 0036 batas berupa jumlah pack (bukan kg) dan ukuran pack tak ikut dihitung. Sejak migrasi 0031 Shopee/Tokopedia juga punya grosir, dan batasnya diatur per produk per kanal (bagian berikut).
- Batas bawaan (pack) diubah owner di Pengaturan › Harga (`GET/PATCH /api/settings/pricing`; GET terbuka untuk semua peran login).
- Layar kasir menghitung ulang harga baris di klien (`terapkanGrosir` di `kasir.astro`, gram bulat agar tak meleset) dan di server saat render; keduanya harus sama dengan backend.
- Impor massal: kolom "ukuran" tetap teks bebas, diurai jadi kg saat produk dibuat (peringatan bila tak terbaca); harga grosir dan harga kanal ikut diimpor sejak migrasi 0032 (lihat bagian harga grosir per kanal).

### Harga grosir per kanal dan batas grosir per produk per kanal (migrasi 0031, 0033, 0036)

- Tiap kanal punya ecer dan grosir sendiri: toko `price`/`price_wholesale`, Shopee `price_shopee`/`price_shopee_wholesale`, Tokopedia/TikTok `price_tiktok`/`price_tiktok_wholesale`.
- Batas grosir **per produk per kanal**: `products.wholesale_threshold_qty` (walk in, dari 0031), `wholesale_threshold_qty_shopee`, `wholesale_threshold_qty_tiktok` (0033), semua nullable. NULL berarti kanal itu memakai batas bawaan `pricing_settings.wholesale_threshold_qty`, jadi produk lama tak berubah saat deploy; batas satu kanal tidak pernah menular ke kanal lain. Migrasi 0033 menyalin batas produk dari 0031 ke dua kolom baru agar produk yang sudah punya batas tetap berharga sama. Batas bawaan tetap diatur di Pengaturan › Harga. Migrasi 0036 mengubah kolom batas dari kg ke jumlah pack (INTEGER): nilai lama dikonversi `FLOOR(kg / ukuran)` (minimal 1) yang setara persis dengan aturan lama, dan produk berukuran bukan 1 kg yang batasnya kosong tetapi punya grosir dipatok ke konversi bawaan lama (10 kg) supaya harganya tak bergeser; bawaan 10 kg jadi 10 pack. Kolom diganti nama `wholesale_threshold_qty*` dan judul kolom edit massal jadi "(pack)".
- Harga kosong: ecer marketplace kosong jatuh ke ecer toko (seperti sebelumnya); grosir kanal kosong jatuh ke ecer **kanal itu**, sengaja tidak ke grosir toko supaya harga marketplace tak turun tanpa diatur.
- Validasi `catalog/service.rs::periksa_harga`: grosir tak boleh lebih tinggi dari ecer kanal yang sama (ecer marketplace kosong dibandingkan dengan ecer toko). Saat menyunting, kolom yang tak dikirim dinilai dengan nilai tersimpannya.
- Impor massal (migrasi 0032) mengenali kolom `harga grosir`, `harga shopee`, `grosir shopee`, `harga tokopedia`, `grosir tokopedia`, `batas grosir`, `batas grosir shopee`, `batas grosir tokopedia` (lihat `import/parse.rs::map_header` untuk sinonimnya). Batas grosir berupa bilangan bulat pack ("20" atau "20 pack", `parse_batas_pack`); desimal dan teks lain jadi WARN dan diperlakukan kosong. Kolom kosong: produk baru memakai harga bawaan (ecer marketplace ikut walk in, grosir tidak ada), produk lama tidak diubah. Grosir yang melebihi ecer kanalnya jadi ERROR saat review bila keduanya terisi di berkas; selebihnya diperiksa `periksa_harga` saat commit.
- Isian harga di form tambah/sunting produk dan tambah varian adalah satu komponen, `web/src/components/IsianHarga.astro`. Hitungan kasir (`hargaKanal`/`pakaiGrosir` di frontmatter dan `terapkanGrosir` di skrip `kasir.astro`) harus tetap sama dengan backend.

### Edit massal produk lewat xlsx (migrasi 0034)

- Tombol "Edit Massal" di `/produk` (khusus owner) membuka `/produk/edit-massal`: unduh xlsx, ubah, unggah, lihat pratinjau, lalu Terapkan. Kode ada di `backend/src/catalog/massal/` (`ekspor`, `rencana`, `terapkan`, `routes`, `repo`), tabel `edit_massal` menyimpan rencana per baris sebagai JSONB sampai diterapkan.
- Dua kelompok. **Utama**: satu baris per produk, dikenali dari kolom `id` lalu SKU; yang tidak ditemukan dibuat sebagai produk baru (wajib nama dan harga ecer walk in, SKU dirakit otomatis, selalu produk tingkat atas, bukan varian). **Stok**: satu baris per batch yang masih bersisa, hanya batch yang sudah ada; sisa stok, kedaluwarsa, rak, harga beli.
- Label pratinjau: Baru, Edit, Tetap, Error. Baris Error dilewati saat Terapkan, baris lain tetap jalan; tiap baris ditulis lewat `catalog::service` dan `stock.rs` (satu baris gagal tak menahan yang lain), hasilnya disimpan di kolom `hasil`.
- Aturan sel: kolom yang tak ada di berkas tidak disentuh; sel kosong pada kolom opsional mengosongkan nilainya; sel kosong pada nama, harga ecer, batas stok, dan status berarti tetap. Ubah SKU lewat `service::koreksi_sku` (ditolak bila produk punya varian atau listing marketplace).
- Sisa stok yang diubah dicatat di ledger sebagai `manual_adjustment` per batch lewat `stock::tambah`/`kurangi`, dan tak boleh melebihi isi kiriman (`quantity`).
- Terapkan berjalan sinkron dalam satu request; berkas dibatasi 5000 baris dan 10 MB. `klaim_sesi` menandai sesi diterapkan lebih dulu agar dua klik tidak menjalankan perubahan dua kali.
- Ekspor mengikuti saringan `cari` dan `kategori` dari daftar produk; tab status (menipis, habis, dll.) belum ikut.

### Diskon ongkir menggantikan diskon umum (migrasi 0023)

- Kasir tak lagi punya input diskon umum; gantinya satu centang "Diskon ongkir: toko yang menanggung" (`shipping_borne_by_store`). `transactions.discount_amount` tetap ada hanya agar omzet transaksi lama tak berubah, dan tak pernah ditulis lagi.
- Dicentang: pembeli tak membayar ongkir (`total_amount` tanpa ongkir) dan ongkir sebenarnya (`shipping_cost`) jadi **beban toko**. Tak dicentang: ongkir ditagih ke pembeli dan bukan beban. Ongkir nol atau kanal Shopee menormalkan penanda ke false.
- Kolom turunan `shipping_charged` dan `shipping_subsidy` (generated, dihitung database) dipakai semua laporan. Subsidi masuk `SalesSummary.expenses`, mengurangi laba, muncul sebagai kategori "Ongkir ditanggung toko" di rincian beban, dan mengurangi `net_profit` transaksinya. Void transaksi otomatis mengeluarkannya dari laporan karena laporan hanya membaca `completed`.
- Beban ini bukan baris di tabel `expenses`; ia diturunkan dari transaksi supaya tak bisa berselisih dengan transaksi yang di-void.

### Biaya Shopee di pengaturan dan kanal marketplace (migrasi 0024)

- Persen komisi (bawaan 17,25%) dan biaya layanan (bawaan 0%) Shopee tak lagi diisi di kasir: owner mengaturnya di Pengaturan › Harga (`PATCH /api/settings/shopee-fees`, pecahan 0..1; GET lewat `/api/settings/pricing`). `checkout` memakainya bila request tak menyebut persen sendiri, dan menyimpan persen itu di transaksi sehingga mengubah pengaturan tak mengubah transaksi lama. PPh 0,5% dan biaya proses Rp1.250 tetap di kode.
- Biaya proses Rp1.250 baru tampil dan dihitung setelah keranjang berisi (sebelumnya "Estimasi Total Penghasilan" kosong tampil -Rp1.250).
- Kanal Tokopedia/TikTok kini berisian sama dengan Shopee: tanpa ongkir, tanpa pilihan metode (dipaksa e-wallet), tanpa "uang diterima". Backend (`SalesChannel::marketplace`) mengabaikan ongkir dan menolak tunai untuk kedua kanal. Potongan Tokopedia ada sejak migrasi 0038 (bagian berikut).

### Potongan Tokopedia / TikTok Shop (migrasi 0038)

- Dasarnya "Detail Penyelesaian Pembayaran" satu pesanan: harga barang Rp60.000 cair Rp46.870, dengan biaya komisi platform 4.650 (7,75%), biaya layanan logistik 3.030 (5,05%), komisi dinamis 3.900 (6,5%), biaya pemrosesan pesanan 1.250, dan PPh 300 (0,5%). Ongkir ditanggung platform dan asuransi Rp500 dibayar pembeli, jadi keduanya di luar hitungan.
- Persen diatur owner di Pengaturan › Harga (`PATCH /api/settings/tiktok-fees`, kolom `pricing_settings.tiktok_*_percent`) dan disimpan per transaksi di `platform_commission_fee_percent`, `platform_dynamic_commission_fee_percent`, `platform_logistics_fee_percent` (dua yang terakhir kolom baru), jadi mengubah pengaturan tak mengubah transaksi lama. PPh 0,5% dan Rp1.250 sama dengan Shopee. Hitungannya satu fungsi, `pos::service::Potongan::hitung`, dan `total_amount` kanal Tokopedia kini uang yang cair, seperti Shopee.
- **Angka logistik 5,05% berasal dari satu contoh** (3.030 dari 60.000); bila di pesanan lain biayanya ternyata tak sebanding harga (mis. bergantung berat paket atau ongkir), cocokkan lalu ubah persennya atau minta kolom tersendiri. Batas maksimum komisi per pesanan, bila ada, belum dimodelkan.
- `transactions.platform_fee_total` (kolom turunan) menjumlahkan keenam potongan dan dipakai ringkasan, ringkasan per kanal, dan laba transaksi di laporan. Transaksi Tokopedia lama (sebelum migrasi) tetap bernilai penuh tanpa potongan.
- Kasir: `PERSEN_KANAL` di `kasir.astro` (dikirim ke skrip lewat `data-persen-kanal`) harus tetap sama dengan `Persen` di `pos/service.rs`.

### Laporan keuangan: beban dan laba rugi (tanpa migrasi)

Dasar risetnya di [`riset-laporan-keuangan.md`](riset-laporan-keuangan.md). `/laporan` kini bertab: Ringkasan, Laba Rugi, Produk, Uang Masuk, Stok, Beban, Void; tab membawa periode aktif.

- **Beban** (`/laporan/beban`, modul `backend/src/expenses/`): sebelumnya tabel `expenses` hanya dibaca laporan dan sheet harian, tak ada jalan untuk mengisinya. Kini `GET/POST /api/expenses`, `GET/PATCH/DELETE /api/expenses/{id}`, khusus owner. Kategori tetap (`KATEGORI` di `expenses/routes.rs`, dikirim ke form lewat `categories` agar satu sumber); "Lainnya" wajib berketerangan dan sama dengan nama laporan untuk kategori kosong. Tanggal tak boleh setelah hari ini (salah ketik tahun menghilangkan beban dari bulan berjalan).
- **Yang sengaja bukan beban:** belanja stok (masuk harga pokok lewat harga beli batch saat terjual), potongan marketplace, PPh, dan ongkir ditanggung toko (semuanya tercatat per transaksi). Mencatatnya di Beban berarti menghitung dua kali; form menjelaskannya.
- **Laba Rugi** (`/laporan/laba-rugi`, `GET /api/reports/profit-loss`): format bertingkat UMKM, Penjualan → HPP → Laba kotor → Biaya penjualan (enam potongan transaksi) → Beban operasional → Laba sebelum pajak → PPh final → Laba bersih, dibanding rentang setara sebelumnya (`previous_from`/`previous_to`). Hanya mengenal periode, bukan metode bayar/jenis pembeli.
- **Invariant:** `LabaRugi.net_profit` = `SalesSummary.profit` untuk periode yang sama. Harga pokok keduanya lewat satu fungsi, `reports::repo::harga_pokok`; rentang periode lewat `reports::rentang_kueri` (dipakai juga oleh daftar beban). Sudah dicocokkan untuk tujuh periode di database lokal.
- **Tren mengikuti periode** (`reports::repo::tren`, menggantikan `monthly_trend` enam bulan tetap): butir dipilih dari panjang rentang (`Butir::untuk`: 1 hari per jam, ≤ 62 hari per hari, selebihnya per bulan). Butir bulan dibulatkan ke tanggal 1 untuk label, tapi transaksi tetap disaring dengan tanggal asli, jadi rentang 15 Mar–20 Okt tak menarik data di luar rentang. Untuk butir hari/bulan jumlah `profit` seluruh batang = `SalesSummary.profit`; butir jam tak memuat tabel beban (dicatat per tanggal), karena itu tabel rincian tak ditampilkan untuk Hari Ini.
- **Produk** (`/laporan/produk`, `GET /api/reports/products?sort=qty|revenue|profit|margin`): `top_products` kini membawa kg, harga pokok, dan laba kotor per produk. Laba di sini sebelum potongan marketplace (melekat pada transaksi), dan Σ laba produk = laba kotor + diskon lama karena diskon transaksi tak dibagi ke barang. Barang tanpa keluaran stok sama sekali (barang lepas, produk terhapus) ditandai HPP belum lengkap.
- **Uang Masuk** (`/laporan/uang-masuk`, `GET /api/reports/cash-in`): Σ `total_amount` per hari/bulan, kanal, dan metode bayar; toko dirinci per metode, marketplace per kanal saja (metode bayarnya selalu e-wallet, tak bermakna). Tanggalnya tanggal pesanan, bukan tanggal dana dilepas marketplace.
- **Kerugian stok (migrasi 0039):** kolom `stock_adjustments.manual_reason` (`rusak`, `kedaluwarsa`, `hilang`, `salah_catat`), dikunci CHECK hanya pada pengurangan manual. Koreksi stok di halaman produk wajib memilih alasan bila mengurangi dan menolak alasan bila menambah (`StockReason::ManualAdjustment(Option<AlasanKoreksi>)` di `stock.rs`). Rusak, kedaluwarsa, dan hilang dinilai `ABS(change_qty) × harga beli batch` (cadangan `products.cost_price`, seperti harga pokok) lewat satu fungsi, `reports::repo::kerugian_stok`, dan dikurangkan dari `SalesSummary.profit`, `LabaRugi` (bagian Kerugian stok, sebelum laba sebelum pajak), dan tren, jadi ketiganya tetap sama. `salah_catat`, koreksi lama, dan edit massal (tanpa alasan) tidak dihitung kerugian. **Laporan harian Google Sheets belum memuat kerugian stok.**
- **Stok** (`/laporan/stok`, `GET /api/reports/stock`): nilai persediaan sekarang, pembelian stok periode (batch yang punya ledger `restock`, yaitu lewat Catat batch atau impor; batch dari koreksi naik dan data seed tak terhitung), kerugian per alasan, dan daftar semua pengurangan manual.
- **Void** (`/laporan/void`, `GET /api/reports/voids`): disaring menurut `voided_at`; jumlah, nilai, persen dari transaksi (selesai + void), siapa yang membatalkan, alasan, dan kasirnya.
- **Cetak:** tombol Cetak / Simpan PDF (JS kosmetik, Ctrl+P tetap jalan). Aturan cetak nota kasir menyembunyikan seluruh `body *`; halaman yang memuat `.lembar-cetak` dikecualikan di `theme.css`, dan kerangka `AppShell` (sidebar, topbar, tab bawah) memakai `print:hidden`.

### Pesanan Shopee dari label pengiriman (migrasi 0027)

API Shopee tidak bisa dipakai, jadi pesanan masuk lewat PDF label ("Cetak
Label" di Seller Centre). Alurnya: owner unggah PDF di `/pesanan` → server
Astro membaca PDF (`web/src/lib/label-shopee.ts`, pdfjs-dist) → backend
mencatatnya (`POST /orders/labels`) → owner mencentang pesanan, memilih batch
per barang dan pengepak (atau dirinya sendiri) → `POST /tickets/bulk` →
notifikasi Web Push ke perangkat pengepak → pengepak "Mulai packing", mencentang
tiap barang, "Barang sudah disiapkan", lalu kirim foto + isi tugas ke WhatsApp
owner → owner ACC di WhatsApp (di luar aplikasi) dan menekan "Selesaikan" di
`/tiket/[id]`. Owner yang mengepak sendiri tak melapor lewat WhatsApp.

- **Membaca label.** Satu label per halaman. Yang diambil: nomor pesanan,
  penerima, alamat, kabupaten dan kecamatan (kotak di bawah alamat), kotak
  pembayaran (CASHLESS/COD), metode Instant/Sameday, batas kirim, dan tabel
  barang (SKU, variasi, qty). Label mencetak "INSTANT / SAMEDAY" dua-duanya;
  yang dipilih ditandai **garis bawah**, yang di PDF berupa bidang isi tipis,
  jadi pembacanya mengikuti CTM dari operator list pdfjs. `pdf-extract` (Rust)
  sempat dicoba dan ditinggalkan: lebar sebagian glyph terbaca 0 sehingga
  posisi kata bergeser dan kolom tak bisa dipisahkan.
- **Telepon penerima tidak ada di label.** Shopee menyembunyikannya; nomor
  yang tercetak di blok "Pengirim" milik toko. Pelanggan dari label dikenali
  lewat nama + alamat (`source = 'marketplace'`), tanpa telepon.
- **Barang dicocokkan lewat SKU** (`products.sku`, tanpa membedakan huruf
  besar-kecil, produk aktif). SKU produk tanpa varian dipakai langsung. SKU
  induk bervarian memilih satu varian anaknya dari teks kolom Variasi label
  (`orders::service::pilih_varian`): teks diurai jadi ukuran (kg atau gr) dan
  kata sisa (merek, grade), lalu dicocokkan ke `variant_size` dan
  `brand_name`/`variant_grade` varian aktif. Hanya satu kecocokan yang
  diterima; variasi kosong, tak dikenal, atau kembar menolak label dengan
  daftar varian yang ada, tak pernah menebak karena salah pilih memotong stok
  barang lain. Nama yang tampil adalah nama di
  app; nama Shopee hanya disimpan di `item_name_snapshot`. Label dengan SKU
  kosong atau tak dikenal ditolak per label, label lain tetap tercatat.
  Nomor pesanan yang sudah ada dijawab "sudah ada", jadi unggah ulang aman.
- **Hanya owner yang menugaskan dan menyelesaikan** (aturannya di
  `tickets::service`): pengepak tak bisa mengambil tugas sendiri,
  dan `packed` → `handed_over` ditolak untuk selain owner karena ACC terjadi
  di WhatsApp. Di UI tugas packing disebut "Tugas Packing"; nama teknisnya
  tetap `tickets`/`/tiket`.
- **Kode pengambilan** (migrasi 0030, `external_orders.pickup_code`) dibaca
  dari "Kode Pengambilan: 9WPS" di label dan menggantikan nomor pesanan di
  layar packing dan Pesanan Masuk; nomor pesanan tetap disimpan untuk laporan.
  Pesanan tanpa kode (label lama, TikTok) menampilkan nomor pesanannya.
- **Batch pilihan owner** disimpan di `ticket_items.batch_id`; kosong = FEFO.
  Rak di tiket mengikuti batch pilihan itu.
- **Penjualan baru tercatat saat tiket selesai** (`handed_over`): lewat
  `pos::service::catat_dari_pesanan`, jalur yang sama dengan kasir (harga
  kanal Shopee, potongan Shopee dari pengaturan, ledger stok `sale`), dalam
  transaksi database yang sama dengan perubahan status tiket. Kunci
  idempotensinya `pesanan:<id pesanan>`, dan pesanannya ditautkan lewat
  `external_orders.transaction_id`. Akibatnya void transaksi itu
  mengembalikan stok ke batch asal seperti void biasa. `StockReason::ExternalOrder`
  dihapus karena tak ada lagi yang memakainya.
- **Web Push**: langganan per perangkat di `push_subscriptions`, dikirim oleh
  server Astro (`web/src/lib/push.ts`, pustaka `web-push`) karena backend
  hanya menyimpan alamatnya. Butuh `VAPID_*` di `web.env`
  (`panduan-deploy.md`). Di iPhone notifikasi hanya jalan bila aplikasi
  ditambahkan ke Layar Utama (iOS 16.4+).
- **Pelanggan** punya `city` (kabupaten/kota) dan `district` (kecamatan),
  terisi otomatis dari label dan bisa disunting di halaman pelanggan.

### Unggah label dari Shortcut iPhone (migrasi 0029)

iOS tak mendukung Web Share Target (bug WebKit 194593), jadi PWA tak bisa
muncul di menu Share iPhone. Gantinya Shortcut iPhone yang mengirim PDF
mentah (`Request Body: File`, bukan Form) ke `POST /unggah-label` di Astro.
Cara memasangnya tertulis di Pengaturan › Shortcut iPhone.

- **Token unggah, bukan sesi.** Shortcut tak membawa cookie, dan JWT sesi
  memberi akses owner penuh selama 30 hari. Owner membuat token di
  Pengaturan; yang disimpan hanya SHA-256-nya (`label_upload_tokens`, satu per
  pengguna, membuat baru = yang lama mati, bisa dicabut). Token hanya diterima
  extractor `PengunggahLabel`, dipakai `GET`/`POST /orders/labels/shortcut`;
  handler yang meminta `CurrentUser` menolaknya, begitu pula sebaliknya. Akun
  yang nonaktif atau bukan owner lagi otomatis kehilangan aksesnya.
- **Token diperiksa sebelum PDF dibaca** (`GET /orders/labels/shortcut`)
  karena pdfjs berat; pemanggil tanpa token sah tak bisa membebani server.
  Badannya (maks. 10 MB) tetap dihabiskan dulu sebelum menjawab galat apa pun:
  jawaban 401/400 yang datang saat iPhone masih mengunggah ditampilkan
  Shortcut sebagai "The network connection was lost", bukan `pesan`-nya.
- **Badan mentah, bukan multipart:** `security.checkOrigin` Astro memeriksa
  header `Origin` pada POST berjenis form, dan Shortcut tak mengirimnya; badan
  `application/pdf` tak tersentuh pemeriksaan itu. `/unggah-label` ada di
  `HALAMAN_PUBLIK` karena ia memeriksa tokennya sendiri.
- Jawabannya selalu `{ pesan }` untuk ditampilkan notifikasi Shortcut.
  Pesanan yang tercatat menunggu di `/pesanan` seperti hasil unggah biasa.

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
- `/pengaturan/platform` sekarang sudah ada halamannya, tapi masih placeholder
  "segera hadir" — belum ada endpoint backend untuk connect/disconnect
  Shopee/TikTok Shop maupun baca status koneksinya dari tabel `platforms`.
- Import dan Mapping produk belum punya endpoint.
- **Kasir belum layak di ponsel.** Sidebar selalu memakan 15rem dan `AppShell`
  tidak punya tombol hamburger maupun breakpoint, jadi di layar 360px hanya
  tersisa ~120px untuk isi. Perbaikan terkecilnya satu media query yang
  memaksa sidebar jadi rel ikon di bawah 768px — tapi itu mengubah tata letak
  semua halaman dan membuat tombol "Ciutkan" tidak berfungsi di layar kecil.
- **Daftar produk di kasir dibatasi 200 baris** (`LIMIT_MAKS`). Begitu katalog
  jual melewati 200, sisanya hanya muncul lewat pencarian; halamannya memberi
  keterangan, bukan paginasi.
- Harga marketplace belum ditampilkan di tabel produk (baru ada di form tambah
  dan halaman produk).
- Batch belum dipakai saat stok berkurang: `quantity` adalah jumlah yang masuk,
  bukan sisa per batch, jadi penjualan belum memotong batch terdekat
  kedaluwarsanya (FEFO). Yang sudah ada: kedaluwarsa terdekat per produk
  tampil di daftar dan ditandai merah bila ≤ 30 hari.
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
- **Image `rust:1.90-bookworm` tanpa rustfmt/clippy**, jadi `scripts/cargo.sh fmt`
  dan `clippy` gagal; jalankan `rustup component add rustfmt clippy` dulu di
  container yang sama (`sh -c "rustup component add ... && cargo clippy ..."`).
- **`cargo sqlx prepare` lewat volume `aj33-sqlx-bin` gagal** ("unrecognized
  subcommand 'sqlx'"). Penggantinya: `cargo clean -p aj33-backend` lalu
  `SQLX_OFFLINE_DIR=<folder kosong> cargo check --all-targets` dengan
  `DATABASE_URL` lokal, kemudian ganti `backend/.sqlx/query-*.json` dengan isi
  folder itu, dan pastikan `SQLX_OFFLINE=true cargo check --all-targets` lulus.

## Perintah verifikasi cepat

```bash
# dari mana saja
curl -s -o /dev/null -w '%{http_code}\n' https://tokoayamaj33.my.id/login

# di VPS
aj33-cek
systemctl is-active aj33-backend aj33-web caddy postgresql
sudo -u postgres psql -d aj33 -tAc "SELECT version, description FROM _sqlx_migrations ORDER BY version"
```

## Laporan harian ke Google Sheets

Apps Script (`docs/apps-script/laporan-harian.gs`) menarik `GET /sheet/harian?tanggal=YYYY-MM-DD` tiap 5 menit dan mengisi satu tab per tanggal dengan layout Offline | Shopee | Tokopedia.

- Alur: Apps Script → Caddy → web `/sheet/harian` (penerus, tanpa cookie) → backend `/api/sheet/harian`. Backend memeriksa `Authorization: Bearer` terhadap env `SHEET_TOKEN` (kosong = endpoint mati, 404); token ini hanya membuka endpoint baca itu.
- Pasang: `openssl rand -hex 32` → `SHEET_TOKEN` di `/etc/aj33/backend.env` (dan `backend/.env` lokal), restart backend; di sheet, Extensions › Apps Script, tempel berkas, isi Script properties `URL_APP` (mis. `https://domain-toko`) dan `TOKEN_SHEET` (nilai yang sama), jalankan `pasangPemicu()` sekali dan setujui izinnya.
- Kolom: Cash = omzet barang (`subtotal - discount`), Item 1..5 = harga pokok per produk (lebih dari 5 produk dijumlahkan ke Item 5), Bersih = `total_amount` (Shopee dan Tokopedia, sudah dipotong platform). Profit offline = Cash − Modal, Shopee dan Tokopedia = Bersih − Modal; PPh offline mengurangi profit baris (di aplikasi PPh toko adalah beban) dan juga ditampilkan terpisah di C2:C3.
- Tab ditimpa tiap sinkron, jadi edit manual di tab hari berjalan akan hilang; hari lampau tidak disentuh.
- Blok tambahan di kanan Tokopedia: Potongan Shopee per pesanan (kolom AJ–AO, sebaris dengan blok Shopee), Pengeluaran hari itu (AQ–AS, dari tabel `expenses` plus subsidi ongkir transaksi), dan Potongan Tokopedia (AU–BA: komisi, komisi dinamis, layanan logistik, PPh, biaya proses, total; definisinya di array `POTONGAN` skrip). Profit offline sudah dikurangi PPh per baris (beban toko, seperti di aplikasi), jadi Profit Bersih (V2) = Total Profit − pengeluaran.
- Nama pelanggan dan Modal berwarna merah bila ada barang tanpa harga beli (`modal_tidak_lengkap`), artinya profit baris itu batas atas.
