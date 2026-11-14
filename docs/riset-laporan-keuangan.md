# Riset tampilan laporan keuangan untuk `/laporan`

Tanggal riset: 13 November 2026. Tujuan: menjadikan `/laporan` laporan keuangan
yang lengkap untuk owner Toko Ayam AJ33 (kasir toko, Shopee, TikTok Shop/Tokopedia).

## 1. Pembanding yang dipelajari

| Aplikasi | Pola yang relevan |
|---|---|
| **Loyverse** (Sales Summary) | Lima kartu metrik: Penjualan Kotor, Refund, Diskon, Penjualan Bersih, Laba Kotor, masing-masing dibandingkan dengan periode setara sebelumnya. Klik kartu → grafik metrik itu. Grafik bisa dikelompokkan per jam/hari/minggu/bulan/kuartal/tahun. Di bawahnya tabel per hari dengan kolom yang bisa disembunyikan, plus ekspor. Definisi: Penjualan Bersih = Kotor − Diskon − Refund; Laba Kotor = Bersih − harga pokok. |
| **majoo** | Ringkasan Penjualan: angka di kiri (total penjualan, laba kotor, pembayaran, refund, transaksi, produk terjual), grafik penjualan per jam di kanan; preset periode + kalender. Menu Laporan terpisah: 10 Teratas, Void, Kasir, Kas Kasir, Produk Terjual, Inventori (Faktur Pembelian, Stok Opname, **Stok Terbuang**). Modul akuntansi: laba rugi, arus kas, neraca. |
| **Opaper** (Laporan Shift) | Tiga blok: Penjualan → Rincian per metode bayar → Arus Kas Tunai. Saldo akhir = Saldo Awal + Buka Kasir + Penjualan − Refund + Kas Masuk Manual − Kas Keluar Manual − Setor/Tutup Kasir. |
| **Qasir** (Rekap Kas) | Modal awal, kas masuk, kas keluar (biaya operasional), penyesuaian saat tutup kasir; dicatat per shift. |
| **Shopify** (Profit by product) | Laba kotor dan marjin per produk: (penjualan bersih − biaya) / penjualan bersih. Hanya produk yang biayanya tercatat, sama seperti peringatan `items_without_cost` AJ33. Tidak sampai laba bersih. |
| **Toast** (restoran) | *Prime cost ratio* (harga pokok + tenaga kerja ÷ penjualan) sebagai satu angka kesehatan utama. Padanan untuk AJ33: % harga pokok terhadap omzet. |
| **Format laba rugi UMKM (SAK EMKM, multi-step)** | Penjualan − HPP = Laba Kotor − Beban Operasional = Laba Usaha − Pajak = Laba Bersih. |

Catatan: dokumentasi publik Moka, Pawoon, dan Olsera tidak merinci isi laporan
keuangannya, jadi tidak dipakai sebagai acuan.

## 2. Keadaan `/laporan` sekarang (dari kode)

Sudah ada, dan sebagian sudah melampaui Loyverse/majoo tingkat dasar:

- 6 kartu: Omzet, Laba, Harga Pokok (marjin kotor), Potongan Marketplace, Order, Kg Terjual; pil naik/turun dibanding periode sebelumnya.
- Omzet per kanal dan **laba per kanal** (dengan rumus terbuka), ditutup beban toko → laba.
- Grafik Omzet vs Beban 6 bulan, Rincian Beban per kategori.
- Produk Terlaris (qty, omzet), daftar Penjualan, Ekspor CSV.
- Peringatan jujur bila harga beli batch belum diisi.

Celah yang ditemukan:

1. **Beban tidak bisa dicatat dari aplikasi.** Tabel `expenses` hanya dibaca
   (laporan dan `/sheet/harian`); tidak ada endpoint maupun form yang menulisnya.
   Akibatnya kartu Laba hanya sebaik data beban yang masuk lewat jalur lain.
2. **Belum ada laporan laba rugi berbentuk pernyataan.** Angkanya tersebar di
   kartu; owner tidak bisa membaca alur Omzet → HPP → Laba Kotor → Potongan → Beban → Laba Bersih dalam satu tempat.
3. **Grafik tidak mengikuti periode.** `monthly_trend` selalu 6 bulan terakhir;
   memilih "Minggu Ini" tidak memberi gambaran per hari. Tidak ada tabel per hari.
4. **Produk terlaris tanpa laba.** Hanya qty dan omzet; tidak ada HPP, laba, marjin, kg per produk.
5. **Tidak ada rincian uang masuk per metode bayar** (tunai/transfer/e-wallet),
   padahal saringannya ada. Owner tidak bisa mencocokkan uang laci dan mutasi rekening.
6. **Void dan diskon tak terlihat.** Transaksi `voided` dan `discount_amount` tidak dilaporkan (majoo punya laporan Void tersendiri; penting untuk kontrol kasir).
7. **Sisi stok belum masuk laporan keuangan:** pembelian stok periode ini
   (`product_batches.purchase_price × quantity`), stok keluar karena penyesuaian
   manual (rusak/kedaluwarsa/susut, dinilai dengan harga beli), nilai persediaan awal/akhir.
   Untuk ayam beku berbatch dengan tanggal kedaluwarsa, ini kebocoran laba yang nyata.

## 3. Rancangan yang direkomendasikan

### Struktur halaman

Tab berupa tautan biasa (tanpa JS), saringan periode/metode/jenis tetap di URL dan terbawa antartab:

```
Laporan
[ Ringkasan ] [ Laba Rugi ] [ Produk ] [ Beban ] [ Uang Masuk ]
Periode: [Bulan Ini ▾]  Dari [__] Sampai [__]  Metode [▾]  Jenis [▾]   [Terapkan] [↓ CSV]
```

- `/laporan` — Ringkasan (yang sekarang, dirapikan)
- `/laporan/laba-rugi` — pernyataan laba rugi
- `/laporan/produk` — laba per produk
- `/laporan/beban` — catat + daftar beban
- `/laporan/uang-masuk` — per metode bayar dan kanal

### Ringkasan (`/laporan`)

- Pertahankan kartu yang ada. Tambah satu angka kesehatan ala *prime cost*: **% HPP terhadap omzet**.
- Grafik mengikuti periode: hari ini → per jam, ≤ 31 hari → per hari, lebih → per bulan (pola Loyverse/majoo). Tetap batang CSS tanpa JS seperti sekarang.
- Tabel per hari di bawah grafik: Tanggal · Order · Omzet · HPP · Laba Kotor · Potongan · Beban · Laba.

### Laba Rugi (`/laporan/laba-rugi`) — prioritas utama

Format multi-step, kolom periode ini vs periode lalu, opsional kolom per kanal:

```
                                   Bulan Ini     Bulan Lalu     Δ
PENJUALAN
  Penjualan kotor                  Rp 52.400.000  Rp 48.100.000  +8,9%
  Diskon                          (Rp    600.000)
  Penjualan bersih                 Rp 51.800.000
HARGA POKOK PENJUALAN             (Rp 41.200.000)
LABA KOTOR                         Rp 10.600.000   marjin 20,5%
POTONGAN MARKETPLACE
  Komisi platform                 (Rp  ...)
  Biaya layanan / logistik        (Rp  ...)
  Biaya proses pesanan            (Rp  ...)
  PPh final 0,5%                  (Rp  ...)
  Ongkir ditanggung toko          (Rp  ...)
LABA SETELAH POTONGAN              Rp  ...
BEBAN OPERASIONAL
  Gaji / upah                     (Rp  ...)
  Listrik & air                   (Rp  ...)
  Kemasan, es, plastik            (Rp  ...)
  Transport / bensin              (Rp  ...)
  Lain-lain                       (Rp  ...)
LABA BERSIH                        Rp  ...   marjin bersih ...%
─────────────────────────────────────────────────────────────
⚠ 12 baris penjualan tanpa harga beli: HPP terlalu kecil, laba terlalu besar.
```

- Pemisahan PPh: PPh toko kini diperlakukan sebagai beban; tampilkan sebagai baris sendiri agar mudah dicocokkan dengan setoran pajak.
- Tambahkan gaya `@media print` agar bisa dicetak/disimpan PDF dari browser tanpa JS.
- Semua angka diambil dari kueri yang sama dengan kartu Ringkasan (satu sumber kebenaran, totalnya harus sama).

### Produk (`/laporan/produk`)

Pola Shopify *Profit by product*: Produk · SKU · Terjual (pack) · Kg · Omzet · HPP · Laba · Marjin %, bisa diurutkan per laba atau marjin. Tandai baris yang HPP-nya belum lengkap. Ini menjawab "produk mana yang laku tapi tipis untungnya".

### Beban (`/laporan/beban`) — wajib agar laba bisa dipercaya

- Form POST biasa: Tanggal · Kategori (pilihan tetap + Lain-lain) · Jumlah · Keterangan.
- Daftar beban periode berjalan dengan total per kategori, ubah/hapus oleh owner.
- Kategori yang disarankan untuk toko ayam: Gaji/Upah, Sewa, Listrik & Air, Kemasan/Es/Plastik, Transport/Bensin, Iklan Marketplace, Peralatan, Lain-lain.

### Uang Masuk (`/laporan/uang-masuk`)

- Pola blok "Rincian per metode bayar" Opaper: Tunai · Transfer · E-wallet · Marketplace, jumlah order dan nilai.
- Untuk marketplace: bruto, potongan, bersih yang akan cair.
- Kas laci per shift (modal awal, kas masuk/keluar manual, setor, selisih) seperti Opaper/Qasir **ditunda**: butuh tabel shift dan alur baru di kasir; layak hanya bila uang tunai di toko besar.

### Tambahan di tahap berikutnya

- **Void & diskon:** jumlah dan nilai transaksi void per kasir, total diskon.
- **Persediaan:** nilai persediaan awal + pembelian − HPP − stok terbuang = nilai akhir; stok terbuang dinilai dari harga beli batch. Perlu alasan penyesuaian yang membedakan rusak/kedaluwarsa/susut (sekarang hanya `manual_adjustment`).

## 4. Urutan pengerjaan yang disarankan

| Prioritas | Pekerjaan | Perubahan |
|---|---|---|
| P0 ✅ | Pencatatan beban (API + halaman) | Selesai 14 Nov 2026: modul `expenses`, `/laporan/beban` |
| P0 ✅ | Laporan Laba Rugi + perbandingan periode + cetak | Selesai 14 Nov 2026: `/reports/profit-loss`, `/laporan/laba-rugi` |
| P1 ✅ | Grafik dan tabel per hari mengikuti periode | Selesai 14 Nov 2026: `reports::repo::tren` |
| P1 ✅ | Laba per produk | Selesai 14 Nov 2026: `/laporan/produk` |
| P1 ✅ | Uang masuk per metode bayar dan kanal | Selesai 14 Nov 2026: `/laporan/uang-masuk` (metode bayar hanya untuk kanal toko) |
| P2 ✅ | Void, pembelian stok, stok terbuang | Selesai 14 Nov 2026: migrasi 0039, `/laporan/stok`, `/laporan/void`; laporan diskon dibatalkan karena diskon umum tak ditulis lagi sejak migrasi 0023 |
| P3 | Kas laci per shift | Tabel dan alur kasir baru |

## Sumber

- [Loyverse – Sales Summary Report in the Back Office](https://help.loyverse.com/help/sales-summary-report-back-office)
- [Loyverse – Reports](https://help.loyverse.com/help/reports)
- [majoo – Panduan Ringkasan Penjualan](https://majoo.id/panduan-pengguna/detail/309)
- [majoo – Aplikasi keuangan](https://majoo.id/aplikasi-keuangan)
- [Opaper – Laporan Shift](https://intercom.help/pt-opaper-international-indonesia/en/articles/11408480-laporan-shift-aplikasi-opaper)
- [Qasir – Rekap Kas](https://blog.qasir.id/en/tips/cash-qasir-recap-easy-ways-to-manage-cash-in-and-cash-out-of-business?region=id)
- [Shopify – Profit reports](https://help.shopify.com/manual/reports-and-analytics/shopify-reports/report-types/profit-reports?ld=1)
- [Toast – Restaurant prime cost](https://pos.toasttab.com/blog/on-the-line/prime-cost)
- [Toast – Restaurant P&L statement](https://pos.toasttab.com/uk/blog/on-the-line/restaurant-profit-and-loss-statement)
- [Square – Sales summary](https://squareup.com/help/us/en/article/5381)
- [Jurnal.id – Cara membuat laporan laba rugi](https://www.jurnal.id/id/blog/cara-membuat-laporan-laba-rugi-perusahaan/)
- [Jurnal.id – Contoh laporan keuangan sederhana UMKM](https://www.jurnal.id/id/blog/contoh-cara-membuat-laporan-keuangan-sederhana-untuk-umkm/)
