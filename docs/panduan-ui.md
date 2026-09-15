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

   **Satu pengecualian yang disetujui: halaman kasir.** Di sana JS dipakai
   untuk tap-to-add, tombol −/+, dan total yang berubah seketika — tanpa itu
   tiap satu barang berarti satu muat-ulang halaman, dan kasir yang berdiri
   di depan pembeli membayar ongkosnya. Aturannya tetap ketat:
   **tanpa JS halaman itu wajib berfungsi penuh**, dan skripnya hanya boleh
   menulis ke isian yang sudah ada di HTML — tidak pernah merakit body
   request, mencegat submit, atau merender baris sendiri. Dengan begitu
   server menerima bentuk data yang sama persis di kedua keadaan. Saklarnya
   `.hanya-js` / `.tanpa-js`, lihat bagian Komponen.
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

### Ukuran

| Token | Nilai | Dipakai untuk |
| --- | --- | --- |
| `--lebar-sidebar` | `15rem` (ciut `4.25rem`) | Lebar sidebar dan margin kolom isi |
| `--tinggi-bilah-tab` | `3.25rem` | Tinggi `.bilah-tab` sekaligus `scroll-margin` tiap bagian — satu angka supaya lompatan `#anchor` tidak berhenti di balik bilahnya |
| `--lebar-borang` | `34rem` | Lebar maksimum satu kolom isian |

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

### Di bawah `md` (768px)

Sidebar berubah jadi **laci geser** yang menutupi isi, bukan kolom di
sampingnya. Kolom isi memakai seluruh lebar layar, dan tombol hamburger muncul
di topbar.

Pemicunya **checkbox tersembunyi + `<label>`, bukan tombol ber-JavaScript.**
Kalau menu hanya bisa dibuka dengan JS, mematikan JS berarti tidak bisa
berpindah halaman sama sekali — kegagalan yang jauh lebih parah daripada
kehilangan animasi. Checkbox-nya `sr-only`, jadi tetap bisa dicapai Tab dan
ditekan Spasi; tirai gelapnya `<label>` kedua yang menutup laci saat diklik.

Tombol "Ciutkan" disembunyikan di sini: lacinya sudah selebar penuh, dan
menciutkannya jadi rel ikon tidak menambah ruang. Lebar laci juga dipaksa
kembali 15rem lewat media query, karena pilihan "ciut" yang tersimpan dari
desktop akan ikut terbawa dan menghasilkan menu ikon tanpa teks.

Padding `<main>` turun jadi `px-4 py-4`. **Semua bilah menempel ikut berubah**
— margin negatifnya harus cocok dengan padding itu, lihat bagian Bilah
menempel.

Aksi utama di bilah bawah dibuat selebar penuh di layar sempit (`flex-1`,
`w-full`) supaya bisa ditekan tanpa membidik.

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

**Satu isian per baris.** Bungkus dengan `.borang-tegak` dan beri tiap isian
kelas `.isian`; angka, tanggal, dan satuan pakai `.isian-pendek`. Keterangan
di bawahnya `.isian-bantu`. Isian bersebelahan membuat mata memindai zig-zag
dan urutan Tab berhenti sama dengan urutan bacanya.

Lebar satu kolom dibatasi `--lebar-borang`. Tanpa batas itu, isian yang
menurun justru lebih sulit dibaca di monitor lebar.

Grid menyamping tetap dipakai untuk **bilah saringan** — itu deretan kontrol
pendek, bukan pengisian data, dan menyusunnya ke bawah mendorong tabel turun
satu layar penuh.

### Remah roti

`.remah` untuk halaman dalam yang punya induk jelas, mis. `Produk › Tambah
Produk Baru`. Topbar hanya memuat judul halaman itu sendiri, jadi jalan
kembalinya harus ditulis di badan halaman. Ruas terakhir ditandai
`aria-current="page"`, pemisahnya `›` dengan `aria-hidden`.

### Bilah tab bagian

`.bilah-tab` berisi `.tab-bagian`, dipakai pada formulir panjang. Isinya
**tautan lompat** ke bagian ber-`id`, bukan panel yang saling menyembunyikan
— seluruh bagian tetap terlihat saat digulir. Tiap bagian diberi
`.sasaran-bagian` supaya judulnya tidak tertutup bilah yang menempel.

**Sengaja tidak ada penanda "tab aktif".** CSS hanya tahu fragmen URL, tidak
pernah tahu posisi gulir; penanda yang mengikuti klik terakhir akan menunjuk
bagian yang salah begitu pengguna menggulir sendiri — lebih buruk daripada
tidak ada penanda, karena salah dengan percaya diri. Mengikuti gulir butuh
`IntersectionObserver`, yaitu JavaScript untuk hiasan semata. Sebagai gantinya
`.sasaran-bagian:target` menyorot bagian tujuannya. **Jangan "memperbaiki" ini
dengan JS.**

### Bilah menempel

| Kelas | Kapan |
| --- | --- |
| `.bilah-aksi` | Tombol simpan formulir panjang, rata kanan, menempel di dasar layar |
| `.bilah-bawah` | Ringkasan + aksi di kasir; tidak rata kanan dan berbayang karena isinya harus terbaca sekilas |

Keduanya membatalkan padding `<main>` di `AppShell` dengan margin negatif —
`-mx-4 -mb-4` di ponsel, `md:-mx-6 md:-mb-6` di layar lebar. **Angkanya harus
selalu sama dengan padding `<main>`**; kalau padding itu berubah dan ini tidak,
bilahnya mengambang dan terlihat seperti kartu yang tersangkut, bukan dasar
layar. `.bilah-tab` memakai aturan yang sama untuk sumbu mendatar.

Kalau satu bilah harus menyimpan form yang bukan induknya — misalnya halaman
yang berisi beberapa form berdiri sendiri — pakai atribut HTML biasa
`form="id-form"` pada tombolnya, jangan JavaScript. Beri label yang menyebut
form mana yang disimpan.

### Komponen kasir

| Kelas | Untuk |
| --- | --- |
| `.langkah` / `.langkah-aktif` / `.langkah-selesai` | Bulatan penunjuk langkah. Angka tetap ditulis, bukan hanya warna |
| `.langkah-penghubung` / `-lewat` | Garis antar bulatan |
| `pil` (@utility) + `.pil-diam` / `.pil-aktif` | Saringan yang ditekan satu ketukan. Berbeda dari `.lencana` yang hanya menampilkan status dan tidak bisa diklik |
| `.baris-produk` / `.baris-produk-terpilih` | Satu barang yang bisa dijual; seluruh blok informasinya target ketuk |
| `.tombol-bulat` | Tombol −/+, 36px — target sentuh terkecil yang masih bisa dikenai jempol tanpa melihat |
| `.kotak-total` + `-angka` | Total yang ditagih di langkah 2, tepat di bawah kolom ongkir dan uang diterima |
| `.kotak-kembalian` + `-angka` | Angka yang dibacakan ke pembeli, sengaja besar |
| `.hanya-js` / `.tanpa-js` | Saklar progressive enhancement |

`.kotak-total` dan `.kotak-kembalian` sengaja seukuran (`text-2xl`) dan
dibedakan hanya oleh warnanya — netral untuk yang ditagih, hijau untuk yang
dikembalikan. Keduanya angka yang diucapkan ke pembeli; membuat salah satunya
lebih kecil membuat yang itu terbaca belakangan, dan urutan baca yang salah di
meja kasir berarti salah sebut nominal.

Di langkah 1, nama barang mendapat satu baris penuh untuk dirinya sendiri dan
kendali jumlah turun ke baris kedua. Sebaris bertiga, nama hanya kebagian sisa
~130px di sel grid selebar 300px. Kendali jumlah dibungkus satu `<div
class="ml-auto">` — `ml-auto` tidak boleh menempel di tombol `−` karena tombol
itu `.hanya-js` dan menghilang saat JavaScript mati, membawa serta perataannya.

`.hanya-js` dan `.tanpa-js` bekerja lewat `data-js` di `:root`, yang dipasang
skrip halaman sebagai baris pertamanya. **Kedua versi selalu ada di HTML dan
selalu ikut terkirim** — yang berubah cuma yang terlihat. Itulah yang membuat
server menerima data yang sama apakah JS hidup atau mati.

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

## Pola halaman formulir panjang

Dipakai `/produk/baru` dan `/produk/[id]`. Urutannya tetap, dari atas ke
bawah:

1. **Remah** — `.remah`, jalan kembali ke induknya
2. **Pesan** — galat atau sukses dari aksi barusan
3. **Bilah tab** — `.bilah-tab`, tautan lompat ke tiap bagian
4. **Kartu bagian** — satu `.kartu.sasaran-bagian` ber-`id` per bagian,
   isinya `.borang-tegak`
5. **Bilah aksi** — `.bilah-aksi`, menempel di dasar layar

Bagian dan tab disetir **satu larik yang sama** di frontmatter, jadi keduanya
tidak bisa lepas sinkron.

Jangan membuat bagian yang isinya belum ada. Kartu kosong adalah janji yang
tidak bisa ditepati aplikasi; lebih baik tabnya tidak ada sama sekali.

Formulir yang gagal validasi **wajib mengembalikan isian yang sudah diketik**
— baca ulang dari `FormData` yang barusan dikirim. Satu galat validasi yang
menghapus belasan isian adalah cara tercepat membuat orang berhenti memakai
halaman itu.

Simpan yang berhasil memakai **POST/redirect/GET** (`303`) ke halaman hasilnya,
supaya refresh tidak membuat data kedua.

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
