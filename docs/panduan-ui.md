# Panduan UI AJ33

Acuan tampilan untuk seluruh halaman. Token dan kelasnya ada di
`web/src/styles/theme.css`; dokumen ini menjelaskan **kapan** memakai yang mana
dan **kenapa**. Kalau menambah token atau komponen di CSS, tambahkan juga
penjelasannya di sini — panduan yang tertinggal dari kodenya lebih berbahaya
daripada tidak ada panduan.

## Prinsip

1. **Layar ini dipakai sambil berdiri dan memegang barang.** Target sentuh
   besar, teks status terbaca dari jarak satu lengan, dan aksi utama tiap
   layar cuma satu.
2. **Warna membawa arti, bukan hiasan.** Tiap warna punya satu tugas. Kalau
   sebuah warna dipakai untuk dua hal berbeda, keduanya berhenti terbaca.
3. **HTML dulu, JavaScript kalau terpaksa.** Semua form bekerja dengan POST
   biasa. JS hanya dipakai kalau tanpa itu fiturnya mustahil (mis. status
   sidebar yang harus bertahan lintas halaman).
4. **Keadaan halaman ada di URL.** Saringan dan paginasi dikirim lewat GET,
   jadi bisa di-bookmark, dibagikan ke rekan, dan tombol "kembali" bekerja.

## Warna

### Dua merah, jangan ditukar

| Peran | Token | Contoh nilai | Dipakai untuk |
| --- | --- | --- | --- |
| **Merah merek** | `merah-50` … `merah-900` | `merah-600` `#b02a20` | Tombol aksi utama, navigasi aktif, kepala tabel, avatar |
| **Merah bahaya** | `red-*` bawaan Tailwind | `red-600` `#dc2626` | Galat, stok menipis, aksi merusak |

Kalau keduanya memakai merah yang sama, tombol "Simpan" tidak bisa dibedakan
dari peringatan — dan pengguna berhenti memperhatikan merah sama sekali.
Merah merek sengaja dipilih gelap (bata tua) supaya teks putih di atasnya
lolos WCAG AA, sekaligus jelas berbeda dari merah peringatan yang terang.

### Netral

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `--color-dasar` | `#f7f7f8` | Latar aplikasi |
| `--color-permukaan` | `#ffffff` | Kartu, sidebar, topbar |
| `--color-garis` | `#e8e6e4` | Tepi kartu dan kolom isian |
| `--color-garis-lembut` | `#f1efee` | Garis antarbaris tabel |
| `--color-teks` | `#1c1917` | Teks utama |
| `--color-teks-lembut` | `#78716c` | Keterangan, kolom sekunder |
| `--color-teks-samar` | `#a8a29e` | Placeholder, judul grup menu |

Latar aplikasi sengaja abu sangat muda supaya kartu putih punya tepi tanpa
perlu bayangan tebal.

### Kuning

Hanya untuk status "menunggu tindakan orang". Luminansinya tinggi, jadi
**selalu** berpasangan dengan teks gelap (`kuning-700` di atas `kuning-100`),
tidak pernah teks putih.

## Tipografi

Font sistem (`ui-sans-serif, system-ui, …`) — tidak ada webfont, supaya
halaman tidak menunggu unduhan font di koneksi toko.

| Peran | Kelas |
| --- | --- |
| Judul halaman | `text-2xl font-bold tracking-tight` (dirender AppShell) |
| Judul kartu | `font-semibold` |
| Teks isi | bawaan, `text-sm` di dalam tabel dan kartu |
| Keterangan | `text-xs text-[var(--color-teks-lembut)]` |
| Kepala kolom tabel | `text-xs font-semibold uppercase tracking-wider` |

**Angka selalu `tabular-nums` dan rata kanan** (kelas `.sel-angka`), supaya
digitnya sejajar dan selisih besaran terlihat dari panjangnya.

## Susunan halaman

```
┌────────────┬──────────────────────────────────────────┐
│  sidebar   │  topbar: judul halaman · identitas user  │
│  (15rem)   ├──────────────────────────────────────────┤
│            │                                          │
│  merek     │  main (px-6 py-6)                        │
│  BISNIS    │                                          │
│  OPERASI…  │                                          │
│            │                                          │
│  Ciutkan   │                                          │
└────────────┴──────────────────────────────────────────┘
```

- **Judul halaman dirender `AppShell` lewat prop `judul`.** Halaman tidak
  boleh menulis `<h1>` sendiri — kalau ada dua, pembaca layar mengumumkan
  halaman punya dua judul.
- **Menu dikelompokkan**: *Bisnis* untuk yang ditata sekali lalu jarang
  disentuh, *Operasional* untuk yang dibuka tiap hari.
- **Menu disaring per peran**, sama persis dengan pembatasan di backend.
  Menu yang tampil tapi ditolak server adalah jebakan.
- **Sidebar bisa diciutkan** jadi 4,25rem (ikon saja). Pilihannya disimpan di
  `localStorage` karena tiap klik menu memuat halaman baru — tanpa disimpan,
  tombolnya tidak ada gunanya.

## Komponen

### Tombol

| Kelas | Kapan |
| --- | --- |
| `.tombol-utama` | Aksi utama. **Satu saja per layar** — kalau ada dua, keduanya berhenti terbaca sebagai "yang ini dulu" |
| `.tombol-halus` | Aksi pendamping yang setara pentingnya, mis. "Import" di sebelah "Tambah" |
| `.tombol-bahaya` | Aksi merusak yang tidak bisa dibatalkan |
| `.aksi-baris` | Aksi di dalam baris tabel. Kecil dan kalem supaya tidak bersaing dengan isi barisnya |

### Wadah

| Kelas | Kapan |
| --- | --- |
| `.kartu` | Wadah umum, sudah berisi padding |
| `.kartu-rapat` | Wadah untuk isi yang mengatur paddingnya sendiri: tabel, daftar berpembatas |

### Kolom isian

`.kolom` untuk `input`/`select`/`textarea`, `.label` untuk labelnya. **Setiap
kolom wajib punya `<label for>`** — placeholder bukan pengganti label, karena
hilang begitu pengguna mulai mengetik.

### Tabel

Bungkus `.kartu-rapat`, tabelnya `.tabel`. Kepala tabel memakai merah merek
pekat dengan teks putih: kontras tinggi itulah yang memisahkan judul kolom
dari ratusan baris di bawahnya, tanpa perlu garis tebal.

Urutan kolom untuk daftar apa pun: **pengenal → nama → atribut → angka →
status → aksi**. Kolom angka pakai `.sel-angka`.

### Lencana status

| Kelas | Arti | Contoh |
| --- | --- | --- |
| `.lencana-perlu-tindakan` | Kuning — menunggu tindakan orang | tiket *Belum diambil* |
| `.lencana-berjalan` | Merah merek — sedang dikerjakan | tiket *Sedang dikemas* |
| `.lencana-selesai` | Hijau — selesai | produk *Aktif*, tiket *Diserahkan* |
| `.lencana-gagal` | Merah bahaya — gagal | pembayaran ditolak |
| `.lencana-nonaktif` | Abu — dimatikan, bukan status kerja | produk *Nonaktif* |

`.lencana-nonaktif` sengaja abu, bukan warna apa pun: "nonaktif" adalah
keadaan yang disengaja, bukan peringatan.

### Pesan

`.galat` untuk kegagalan (selalu dengan `role="alert"`), `.sukses` untuk
hasil yang berhasil. **Pesan dari backend ditampilkan apa adanya** — pesan itu
sudah ditulis untuk pengguna dan tahu konteksnya; menggantinya dengan pesan
generik buatan frontend justru menghilangkan informasi.

## Pola halaman daftar

Urutannya tetap, dari atas ke bawah:

1. **Aksi** — tombol tambah/import
2. **Pesan** — galat atau sukses dari aksi barusan
3. **Saringan** — di dalam `.kartu`, dikirim `method="GET"`
4. **Tabel** — di dalam `.kartu-rapat`
5. **Paginasi** — "Menampilkan *x*–*y* dari *n*" di kiri, tombol halaman di kanan

Saringan memakai GET, bukan POST: hasilnya tercermin di URL. Ganti saringan
selalu mengembalikan ke halaman 1 (parameter `halaman` tidak ikut dikirim form
saringan); tautan paginasi membawa seluruh saringan yang sedang aktif.

## Aksesibilitas

Yang tidak boleh dilewat:

- Fokus keyboard selalu terlihat (`:focus-visible` global, jangan ditimpa).
- Setiap kolom isian punya `<label for>`.
- Ikon dekoratif diberi `aria-hidden="true"`; ikon yang berdiri sendiri
  sebagai tombol wajib punya `aria-label`.
- Menu aktif ditandai `aria-current="page"`, bukan hanya warna.
- Status tidak pernah disampaikan lewat warna saja — lencana selalu berisi
  teks statusnya.
- Kontras: merah merek `600`/`800` dengan teks putih, kuning dengan teks
  gelap. Jangan membalik pasangan ini.

## Menambah komponen baru

Sebelum menulis kelas baru, periksa berurutan:

1. Sudah ada kelas yang mengerjakannya? Pakai itu.
2. Bisa jadi varian dari `@utility tombol` / `@utility lencana`? Tambahkan
   varian, jangan menyalin deklarasi dasarnya.
3. Betul-betul baru? Tulis di `@layer components` pada `theme.css`, beri
   komentar alasannya, lalu catat di dokumen ini.

Catatan Tailwind v4: `@apply` hanya menerima *utility*, bukan kelas komponen
buatan sendiri. Karena itu `tombol` dan `lencana` dideklarasikan dengan
`@utility` — supaya varian di bawahnya bisa membangun di atasnya.

Catatan parser Astro: `<=` di dalam ekspresi template dibaca sebagai awal tag
Fragment dan menggagalkan build. Tulis terbalik — `batas >= stok`, bukan
`stok <= batas`.
