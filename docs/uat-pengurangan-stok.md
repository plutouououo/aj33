# UAT Pengurangan Stok Aplikasi POS

| Keterangan | Isi |
|---|---|
| Versi dokumen | 1.1 (diselaraskan dengan perbaikan stok tahap 1 dan 2a) |
| Tanggal uji | ____________________ |
| Lingkungan lokal | URL: ____________________ Commit: ____________________ |
| Lingkungan staging | URL: ____________________ Commit: ____________________ |
| Nama penguji | ____________________ |

---

## 1. Tujuan dan cakupan

**Tujuan.** Memastikan setiap kejadian yang mengubah stok di aplikasi tercatat benar: stok total produk, sisa tiap batch, dan catatannya di laporan. Dokumen ini diuji lewat tampilan aplikasi, bukan lewat program.

**Sumber perubahan stok yang diuji:**

| Kode | Sumber | Keterangan singkat |
|---|---|---|
| A | Penjualan kasir | Stok turun saat kasir menyelesaikan penjualan di halaman kasir |
| B | Pembatalan transaksi (void) | Stok kembali ke batch asal saat owner membatalkan transaksi |
| C | Tugas packing dari label Shopee | Stok turun hanya saat owner menekan "Selesaikan" di tiket |
| D | Koreksi stok manual | Owner menambah atau mengurangi stok, dengan alasan |
| E | Batalkan batch | Owner menarik kembali seluruh isi sebuah batch |
| F | Edit massal atribut stok | Owner mengubah sisa stok lewat berkas xlsx |
| G | Barang masuk | Catat batch dan stok awal produk (dicakup lewat BAT-01 dan BAT-04) |
| H | Hapus produk | Owner menghapus produk uji beserta stoknya (PRD-01) |

**Di luar cakupan:** sinkronisasi API Shopee dan TikTok serta notifikasi push. Keduanya tidak mengubah stok, dan API tidak punya tampilan yang bisa diuji. Keduanya hanya dicatat di Bagian 7.

---

## 2. Prasyarat

### 2.1 Lingkungan lokal (untuk kasir, void, koreksi, batch, edit massal, hapus produk)

1. Developer menyalakan database lokal (Docker) dan memastikan aplikasi menunjuk database lokal di port 5433, bukan database jarak jauh.
2. Developer menjalankan backend dan web, lalu menjalankan skrip seed (`scripts/seed-dev.sh`) pada database yang kosong.
3. Akun uji: owner / owner123, kasir / kasir123, pengepak / pengepak123. Nama tampilan owner "Owner Toko", pengepak "Pengepak Gudang".
4. Siapkan tiga profil browser atau jendela penyamaran, masing-masing untuk satu peran (owner, kasir, pengepak).
5. Seed tidak mengembalikan stok bila dijalankan ulang. Untuk mengulang UAT dari awal, developer mengosongkan database, lalu menjalankan migrasi dan seed lagi.

### 2.2 Lingkungan staging (untuk alur tugas packing dari label PDF)

1. Akun owner, kasir, dan pengepak tersedia di staging.
2. Produk aktif dengan SKU yang sama persis dengan SKU di label harus ada: **FPKA-AFC-1KG**, **CBRA-AFC-1KG**, dan **MDMA-AFC-1KG**. Ketiganya tidak ada di data seed.
3. Setiap produk di atas punya stok awal. Dalam dokumen ini disebut **S1** (FPKA), **S2** (CBRA), dan **S3** (MDMA). Penguji memilih angkanya sendiri, misalnya 10.
4. Produk **CBRA-AFC-1KG** dibuat dengan dua batch berbeda tanggal kedaluwarsa: **CBRA-A** (kedaluwarsa lebih awal) dan **CBRA-B** (kedaluwarsa lebih jauh). Batch dicatat lewat "Catat batch" atau "Stok awal".
5. Tiga berkas PDF dari folder "test label" sudah ada di komputer penguji (lihat Bagian 3.2).
6. Staging dipakai hanya untuk uji ini. Jangan pernah menjalankan dokumen ini di produksi.

---

## 3. Data uji

### 3.1 Data seed lokal

| SKU | Nama produk | Stok | Batch (sisa, kedaluwarsa) | Harga ecer | Harga pokok |
|---|---|---|---|---|---|
| CBSB-AFC-2KG | CBSB ceker bersih super besar | 24 | CBSB-2508 (sisa 24, 25-08-2027) | 28.000 | 21.000 |
| HJA-AFC-1KG | HJA hati jantung ampela | 16 | HJA-2511 (sisa 16, 30-11-2027) | 19.500 | 14.000 |
| AU08-AFC-2KG | Ayam utuh SP 08 | 10 | SP08-2906 (sisa 4, 29-06-2027) dan SP08-2507 (sisa 6, 25-07-2027) | 52.000 | 41.000 |
| FDU-BEC-2KG | Filet dada utuh pack 2 kg | 6 | FDU-2604 (sisa 6, 30-04-2027) | 78.000 | 63.000 |

### 3.2 Data berkas label Shopee untuk uji packing (staging)

Hanya SKU, nama produk, teks Variasi, dan jumlah (qty) yang dicatat di sini. Data pribadi pembeli dan nomor pesanan tidak dimuat dalam dokumen.

| Berkas | Nama produk di label | SKU | Variasi | Qty | Dipakai di skenario |
|---|---|---|---|---|---|
| Label Pengiriman.pdf | Filet Paha Dengan Kulit 1 KG Frozen | FPKA-AFC-1KG | (kosong, tanpa varian) | 1 | PCK-09, PCK-01, PCK-05, PCK-10 |
| Label Pengiriman(2).pdf | Ceker Tanpa Tulang 990 Gram Frozen Reguler (rebus), Bersih, Produk Berlisensi HALAL & NKV, Vacuum - Pengiriman Express | CBRA-AFC-1KG | (kosong, tanpa varian) | 1 | PCK-02, PCK-06, PCK-08, PCK-07 |
| Label Pengiriman(3).pdf | MDM Daging Ayam Giling Kualitas Baik 1 KG Frozen, Produk Berlisensi HALAL & NKV | MDMA-AFC-1KG | (kosong, tanpa varian) | 1 | PCK-03, PCK-04 |

Catatan pembacaan:

- Pada ketiga label, SKU tercetak terbelah dua baris. Contohnya "FPKA-AFC-1K" di baris atas dan "G" di baris bawah. SKU lengkapnya ditulis utuh di tabel di atas.
- Ketiga label tanpa varian. Tidak ada label bervarian di folder uji, sehingga **PCK-11 tidak bisa diuji** dengan data ini.
- Ketiga label berisi satu barang dengan qty 1. Tidak ada label multi-barang atau qty lebih dari 1 di folder uji, sehingga pengujian dengan qty lebih dari 1 pada alur packing **belum tercakup**. Penguji dapat menambah label seperti itu bila tersedia.

### 3.3 Urutan pelaksanaan alur packing

Ketiga berkas hanya dipakai sekali untuk pesanan baru, jadi beberapa skenario packing harus dijalankan berurutan pada berkas yang sama:

1. PCK-09 pada berkas FPKA (bagian gagal dan pemulihan), lalu PCK-01 pada berkas FPKA.
2. PCK-05 (unggah ulang berkas FPKA setelah PCK-01).
3. PCK-03 dan PCK-04 pada berkas MDMA.
4. PCK-02, PCK-06, PCK-08, dan PCK-07 pada tiket berkas CBRA. Urutannya: PCK-02 (pilih batch, kirim, mulai packing), PCK-06 (coba selesai sebelum semua dicentang), centang semua barang, PCK-08 (pengepak memeriksa tiket), lalu PCK-07 (dua tab owner). Selesaikan di tab pertama adalah hasil PCK-02.
5. PCK-10 (void transaksi hasil PCK-01).

---

## 4. Cara membaca stok dan aturan pencatatan

1. **Stok produk = jumlah sisa semua batchnya.** Angka stok total dilihat di halaman /produk pada kolom "Stok". Di halaman detail produk (/produk/[id]) bagian "Batch & Stok" tidak ada angka total. Jumlahkan kolom "Sisa" dari semua baris batch.
2. **Di halaman kasir** (/kasir) dengan JavaScript aktif, tombol "i" pada kartu produk menampilkan "Stok: N · k batch" dan tiap batch "sisa N · tanggal". Tanpa JavaScript, kartu menampilkan "SKU · stok N".
3. **Laporan stok** (/laporan/stok) menampilkan Nilai Persediaan Sekarang, Pembelian Stok, Kerugian Stok, dan tabel "Pengurangan Stok Manual".
4. **Riwayat mutasi tidak tampil di aplikasi.** Bila perlu memastikan detail mutasi, minta developer menjalankan SQL di Lampiran a.
5. **Catat stok sebelum dan sesudah** di kolom yang tersedia. Untuk skenario yang menyentuh batch, catat juga sisa batch yang disebut.
6. **Tangkapan layar** bila hasil tidak sesuai. Beri nama file dengan ID skenario, misalnya "KSR-02-gagal.png".
7. **Angka contoh** mengasumsikan data seed segar dan urutan skenario dijalankan berurutan. Bila hasil sebelumnya berbeda, bandingkan selisihnya (naik atau turun berapa), bukan angka mutlaknya. Skenario yang memakai angka relatif ditulis dengan S, S1, S2, atau S3.
8. **Batch yang dipakai otomatis (FEFO):** batch dengan tanggal kedaluwarsa paling dekat keluar lebih dulu. Ini berlaku untuk penjualan dengan pilihan "Otomatis" dan untuk koreksi stok turun.
9. **Nilai di laporan** = jumlah x harga beli batch. Data seed yang tidak punya harga beli memakai harga pokok produk.
10. **Status Lulus/Gagal:**
    - "Lulus" bila hasil sama dengan kolom Hasil yang diharapkan.
    - "Gagal" bila berbeda.
    - "dicatat saja" untuk perilaku saat ini (lihat Catatan di skenario bertanda itu). Perilaku ini tidak dihitung gagal, tetapi wajib dicatat.

---

## 5. Skenario uji

Semua skenario dijalankan dengan urutan seperti tabel, kecuali bagian packing yang mengikuti Bagian 3.3.

### 5.1 Penjualan kasir (sumber A)

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| KSR-01 | Kasir | Login sebagai kasir. CBSB stok 24, batch CBSB-2508 sisa 24. | 1. Buka /kasir.<br>2. Ketuk kartu "CBSB ceker bersih super besar" sampai jumlahnya 2.<br>3. Pilih kanal "Toko" dan metode "Tunai".<br>4. Isi "Uang diterima (tunai)" dengan 56.000 atau lebih.<br>5. Tekan "Selesaikan · Rp…". | Struk tampil. Stok CBSB turun 2 menjadi 22. Batch CBSB-2508 sisa 22. Transaksi muncul di /laporan pada tabel "Penjualan". | | | | |
| KSR-02 | Kasir | Login sebagai kasir. AU08 stok 10: SP08-2906 sisa 4 dan SP08-2507 sisa 6. | 1. Buka /kasir.<br>2. Ketuk kartu "Ayam utuh SP 08", lalu di dialog batch isi SP08-2906 jumlah 4 dan SP08-2507 jumlah 1.<br>3. Pilih kanal "Toko" dan metode "Transfer".<br>4. Tekan "Selesaikan · Rp…". | Stok AU08 turun dari 10 menjadi 5. SP08-2906 berstatus "habis". SP08-2507 sisa 5. | | | | |
| KSR-03 | Kasir | Login sebagai kasir. HJA stok 16. | 1. Ketuk kartu "HJA hati jantung ampela" sampai jumlahnya 1.<br>2. Pilih kanal "Shopee".<br>3. Periksa bahwa metode terkunci di "E-wallet" dan tidak bisa diubah.<br>4. Isi "Nomor pesanan marketplace" dengan "UAT-SP-001".<br>5. Tekan "Selesaikan · Rp…". | Penjualan berhasil. Stok HJA turun dari 16 menjadi 15. | | | | |
| KSR-04 | Kasir | Login sebagai kasir. FDU stok 6. | 1. Ketuk kartu "Filet dada utuh pack 2 kg" sampai jumlahnya 1.<br>2. Pilih kanal "Tokopedia / TikTok".<br>3. Periksa bahwa metode terkunci di "E-wallet".<br>4. Isi "Nomor pesanan marketplace" dengan "UAT-TT-001".<br>5. Tekan "Selesaikan · Rp…". | Penjualan berhasil. Stok FDU turun dari 6 menjadi 5. Struk tampil. | | | | |
| KSR-05 | Kasir | Struk dari KSR-04 masih terbuka di browser. | 1. Di halaman struk, tekan F5.<br>2. Setujui kirim ulang formulir pada dialog browser. | Struk dan nomornya sama seperti sebelumnya. Stok FDU tetap 5. Tidak ada transaksi baru di /laporan. | | | | |
| KSR-06 | Kasir (dua tab) | Stok FDU 5. Dua tab /kasir terbuka, keduanya memuat FDU dengan jumlah 5. | 1. Di tab 1, selesaikan penjualan FDU jumlah 5.<br>2. Di tab 2 (yang sudah terbuka sebelum tab 1 selesai), tekan "Selesaikan · Rp…".<br>3. Muat ulang /kasir. | Tab 1 berhasil dan stok FDU menjadi 0.<br>Tab 2 menampilkan `Batch FDU-2604 untuk "Filet dada utuh pack 2 kg" tinggal 0, tidak cukup untuk 5.`<br>Tidak ada transaksi kedua. FDU tidak tampil lagi di /kasir. | | | | |
| KSR-07 | Kasir | Stok CBSB 22 (hasil KSR-01). | 1. Di /kasir, tambahkan CBSB ke keranjang.<br>2. Di isian jumlah, coba masukkan 23 (lebih dari sisa 22). | Browser menolak nilai 23. Tidak ada transaksi. Stok CBSB tetap 22. | | | | |
| KSR-08 | Kasir | Keranjang berisi CBSB 1 (Rp28.000). | 1. Pilih metode "Tunai".<br>2. Isi "Uang diterima (tunai)" dengan 20.000.<br>3. Tekan "Selesaikan · Rp…". | Muncul pesan `Nominal pembayaran kurang dari total belanja.` Stok tetap. Tidak ada transaksi. | | | | |
| KSR-09 | Pengepak | Login sebagai pengepak. | 1. Buka /kasir. | Halaman menampilkan stok produk, tetapi tidak ada keranjang dan tidak ada tombol "Selesaikan". | | | | |
| KSR-10 | Kasir (JavaScript dimatikan di browser) | Login sebagai kasir. Stok CBSB 22 (atau sesuai hasil sebelumnya). | 1. Matikan JavaScript di browser penguji.<br>2. Buka /kasir.<br>3. Di kartu CBSB, isi jumlah 1, lalu tekan "Perbarui keranjang".<br>4. Pilih metode "Tunai", isi uang diterima yang cukup, lalu tekan "Selesaikan".<br>5. Nyalakan kembali JavaScript. | Keranjang diperbarui. Transaksi berhasil. Stok CBSB turun sesuai jumlah (turun 1 dari angka sebelumnya). | | | | |
| KSR-11 | Kasir | Login sebagai kasir. Penjualan KSR-03 (nomor UAT-SP-001) masih berlaku. HJA stok 15. | 1. Di /kasir pilih kanal "Shopee", ketuk HJA jumlah 1.<br>2. Biarkan "Nomor pesanan marketplace" kosong, tekan "Selesaikan".<br>3. Isi nomor " uat-sp-001 " (huruf kecil dan spasi), tekan "Selesaikan" lagi. | Langkah 2 ditolak dengan pesan `Nomor pesanan marketplace wajib diisi untuk penjualan Shopee atau Tokopedia / TikTok.`<br>Langkah 3 ditolak dengan pesan `Pesanan UAT-SP-001 sudah dicatat di transaksi kasir yang belum dibatalkan.` Stok HJA tetap 15. | | | | Nomor dinormalkan (spasi dibuang, huruf besar) sebelum dibandingkan. |
| KSR-12 | Owner, kasir | Penjualan KSR-03 ada. | 1. Owner: void transaksi KSR-03 dengan alasan "uji nomor".<br>2. Kasir: catat ulang HJA kanal Shopee dengan nomor "UAT-SP-001". | Penjualan baru berhasil karena transaksi yang di-void melepas nomornya. Stok HJA turun 1 dari angka setelah void. | | | | |
| KSR-13 | Kasir | Pesanan Shopee dengan nomor tertentu sudah ada di /pesanan (dari label atau sinkron). | 1. Di /kasir pilih kanal "Shopee" lalu isi nomor pesanan itu.<br>2. Tekan "Selesaikan". | Ditolak dengan pesan `Pesanan <nomor> sudah tercatat dari label atau sinkron marketplace. Proses lewat tugas packing, jangan dicatat sebagai penjualan kasir.` Stok tidak berubah. | | | | Nomor yang sama di kanal TikTok tidak bentrok karena beda platform. |

### 5.2 Pembatalan transaksi / void (sumber B)

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| VOID-01 | Owner | Transaksi KSR-01 belum dibatalkan. | 1. Login sebagai owner.<br>2. Buka /laporan, lalu klik nomor pesanan KSR-01.<br>3. Di kotak "Batalkan Transaksi", isi "Alasan pembatalan" dengan "uji void".<br>4. Tekan "Batalkan Transaksi".<br>5. Setujui dialog konfirmasi. | Lencana "Dibatalkan" dan tulisan "Alasan: uji void" tampil. Stok CBSB naik 2 dan kembali ke batch asal CBSB-2508. Transaksi muncul di /laporan/void, tabel "Daftar Void", dengan kolom Dibatalkan, Oleh, dan Alasan. | | | | |
| VOID-02 | Owner | Transaksi KSR-02 belum dibatalkan. AU08: SP08-2906 sisa 0, SP08-2507 sisa 5. | 1. Buka transaksi KSR-02 di /laporan.<br>2. Isi alasan, lalu tekan "Batalkan Transaksi" dan setujui dialog. | Stok AU08 naik 5 (total 10). SP08-2906 naik 4 menjadi sisa 4. SP08-2507 naik 1 menjadi sisa 6. Stok kembali ke batch asalnya. | | | | |
| VOID-03 | Owner (dua tab) | Transaksi KSR-04 (FDU) belum dibatalkan. Halaman transaksi dibuka di tab 1 dan tab 2. | 1. Di tab 1, isi alasan "uji void ganda", tekan "Batalkan Transaksi", lalu setujui.<br>2. Di tab 2 (dibuka sebelum tab 1 selesai), isi alasan, tekan "Batalkan Transaksi", lalu setujui. | Tab 1 berhasil. Tab 2 menampilkan `Transaksi ini sudah dibatalkan.` Stok FDU naik 1 sekali saja. | | | | |
| VOID-04 | Owner | Transaksi aktif dari KSR-06 (tab 1) masih ada. | 1. Buka halaman transaksi tersebut.<br>2. Kosongkan "Alasan pembatalan".<br>3. Tekan "Batalkan Transaksi". | Browser menahan pengiriman karena isian wajib kosong. Tidak ada perubahan pada transaksi dan stok. | | | | |
| VOID-05 | Kasir | Login sebagai kasir. | 1. Buka /laporan secara langsung di alamat browser. | Kasir dialihkan ke /kasir. | | | | |

### 5.3 Koreksi stok manual (sumber D)

Prakondisi umum: HJA stok 15 setelah KSR-03. Semua langkah dilakukan oleh owner di halaman /produk/[id] produk HJA, bagian "Batch & Stok", judul "Koreksi stok". Tombol "Koreksi stok".

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| KOR-01 | Owner | HJA stok 15. | 1. Isi "Selisih" dengan -2.<br>2. Pilih alasan "Rusak".<br>3. Tekan "Koreksi stok". | Muncul "Stok dikoreksi." Stok HJA menjadi 13. Di /laporan/stok, tabel "Pengurangan Stok Manual" mencatat baris dengan Alasan "Rusak", Nilai Rp28.000, dan Oleh "Owner Toko". Kerugian Stok naik Rp28.000. Halaman Laba Rugi menampilkan kerugian ini. | | | | |
| KOR-02 | Owner | Stok HJA 13 (hasil KOR-01). | 1. Isi "Selisih" dengan -1.<br>2. Pilih alasan "Kedaluwarsa".<br>3. Tekan "Koreksi stok". | Muncul "Stok dikoreksi." Stok HJA menjadi 12. Baris baru di "Pengurangan Stok Manual" dengan alasan "Kedaluwarsa". Kerugian Stok bertambah. | | | | |
| KOR-03 | Owner | Stok HJA 12. | 1. Isi "Selisih" dengan -1.<br>2. Pilih alasan "Hilang (selisih opname)".<br>3. Tekan "Koreksi stok". | Muncul "Stok dikoreksi." Stok HJA menjadi 11. Baris baru dengan alasan "Hilang (selisih opname)". Kerugian Stok bertambah. | | | | |
| KOR-04 | Owner | Stok HJA 11. | 1. Isi "Selisih" dengan -1.<br>2. Pilih alasan "Salah catat (bukan kerugian)".<br>3. Tekan "Koreksi stok". | Muncul "Stok dikoreksi." Stok HJA menjadi 10. Baris muncul di "Pengurangan Stok Manual". Kerugian Stok tidak bertambah. | | | | |
| KOR-05 | Owner | Stok HJA 10. | 1. Isi "Selisih" dengan -2.<br>2. Biarkan alasan kosong.<br>3. Tekan "Koreksi stok". | Muncul pesan `Pilih alasan pengurangan stok: rusak, kedaluwarsa, hilang, atau salah catat.` Stok tetap 10. | | | | |
| KOR-06 | Owner | Stok HJA 10. | 1. Pilih alasan "Rusak" (biarkan terpilih).<br>2. Isi "Selisih" dengan 3.<br>3. Tekan "Koreksi stok". | Muncul "Stok dikoreksi." Stok HJA naik 3. Muncul batch baru tanpa nomor dan tanpa kedaluwarsa (tampil "—") dengan sisa 3. Alasan yang terpilih diabaikan: baris ini tidak masuk "Pengurangan Stok Manual" dan Kerugian Stok tidak berubah. | | | | |
| KOR-07 | Owner | Stok HJA 13. | 1. Pilih alasan apa saja.<br>2. Isi "Selisih" dengan 0.<br>3. Tekan "Koreksi stok". | Muncul pesan `Perubahan stok tidak boleh nol.` Stok tetap. | | | | |
| KOR-08 | Owner | Stok HJA N (N = stok saat itu, misalnya 13). | 1. Pilih alasan "Rusak".<br>2. Isi "Selisih" dengan -999.<br>3. Tekan "Koreksi stok". | Muncul pesan `Stok "HJA hati jantung ampela" tinggal N, tidak cukup untuk 999.` Stok tetap. | | | | |
| KOR-09 | Owner | VOID-02 sudah dijalankan: AU08 dengan SP08-2906 sisa 4 dan SP08-2507 sisa 6 (total 10). | 1. Buka /produk/[id] produk AU08.<br>2. Isi "Selisih" dengan -5 dan pilih alasan "Rusak".<br>3. Tekan "Koreksi stok". | Stok AU08 turun dari 10 menjadi 5. SP08-2906 dikurangi dulu sampai habis (4 menjadi 0), sisanya 1 diambil dari SP08-2507 (6 menjadi 5). Di "Pengurangan Stok Manual" tercatat jumlah 5. Nilai dihitung dari harga beli batch yang berkurang. Dengan harga pokok seed 41.000 hasilnya Rp205.000. | | | | |
| KOR-10 | Owner | Produk induk bervarian sudah dibuat di lingkungan lokal (penyiapan dilakukan developer; ikuti cara membuat varian di menu produk). | 1. Buka /produk/[id] produk induk bervarian. | Form "Koreksi stok" tidak tampil. Muncul teks "Produk ini punya varian…" sebagai ganti form. | | | | |
| KOR-11 | Owner | Stok HJA S. | 1. Isi "Selisih" -1 dengan alasan "Rusak".<br>2. Tekan "Koreksi stok" dua kali cepat, atau sekali lalu Back dan kirim lagi.<br>3. Buka /laporan/stok. | Stok HJA turun 1, hanya sekali. Tabel "Pengurangan Stok Manual" hanya menambah satu baris. | | | | | Dijaga kunci kiriman per form. |

### 5.4 Batch (sumber E dan G)

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| BAT-01 | Owner | HJA stok 13 (hasil KOR-06). | 1. Buka /produk/[id] produk HJA.<br>2. Di "Catat batch", isi nomor batch "UAT-01" dan jumlah 5.<br>3. Tekan "Catat batch". | Stok HJA naik 5. Batch baru muncul di /laporan/stok pada tabel "Pembelian Stok". | | | | |
| BAT-02 | Owner | Batch UAT-01 utuh (sisa 5). | 1. Buka /produk/[id] produk HJA, bagian "Batch & Stok".<br>2. Di baris UAT-01, kolom Aksi, tekan tombol merah "Batalkan". Tidak ada konfirmasi tambahan. | Muncul "Batch dibatalkan dan stoknya ditarik kembali." Stok HJA turun 5. Baris UAT-01 hilang. Batch ini tidak muncul di "Pengurangan Stok Manual". | | | | |
| BAT-03 | Owner dan kasir | Owner sudah mencatat batch "UAT-02" dengan jumlah 5 untuk HJA. Kasir siap menjual. | 1. Owner: catat batch UAT-02 jumlah 5 (sebagai bagian prakondisi).<br>2. Kasir: di /kasir, ketuk HJA, lalu di dialog batch pilih UAT-02 dengan jumlah 1. Pilih kanal "Toko" dan metode "Tunai", lalu selesaikan.<br>3. Owner: di /produk/[id] produk HJA, tekan "Batalkan" pada baris UAT-02. | Pembatalan ditolak dengan pesan yang berakhiran `tinggal 4, tidak cukup untuk 5.`. Batch UAT-02 tetap ada dengan sisa 4. Stok HJA turun 1 akibat penjualan langkah 2 dan tetap di angka itu setelah pembatalan ditolak. | | | | |
| BAT-04 | Owner | Batch UAT-02 masih ada untuk produk HJA. | 1. Di /produk/[id] produk HJA, bagian "Catat batch", isi nomor batch "UAT-02" dan jumlah 1.<br>2. Tekan "Catat batch". | Muncul pesan `Batch "UAT-02" sudah pernah dicatat untuk produk ini.` Stok tidak berubah. Tidak ada batch ganda. | | | | |
| BAT-05 | Owner | Stok HJA S. Form "Catat batch" terbuka dengan nomor batch "UAT-05" dan jumlah 5. | 1. Tekan "Catat batch" dua kali cepat, atau tekan sekali lalu tombol Back browser dan kirim form yang sama lagi.<br>2. Buka /produk/[id] produk HJA. | Stok HJA naik 5, hanya sekali. Hanya satu batch UAT-05. Tidak ada galat 500. Halaman tidak dipulihkan dari cache saat Back (form dimuat ulang dengan kunci baru). | | | | | Dijaga kunci kiriman per form (`Idempotency-Key`) dan `Cache-Control: no-store`. |

Catatan BAT-04: nomor yang dipakai adalah UAT-02 (bukan UAT-01 seperti rancangan awal), karena UAT-01 sudah ditarik di BAT-02. Nomor batch yang ada harus dipakai agar pesan duplikat muncul.

### 5.5 Tugas packing dari label Shopee (sumber C, staging)

Semua skenario di bagian ini dilakukan di **staging**. Angka S1, S2, dan S3 adalah stok awal masing-masing produk. Urutan pelaksanaan ada di Bagian 3.3. Alur normal: owner unggah label di /pesanan, lalu tekan "Unggah Label Shopee" > pilih PDF > "Baca label". Setelah itu pesanan dicentang, lalu owner memilih "Tugaskan ke" dan "Ambil dari batch", dan menekan "Kirim tugas packing terpilih".

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| PCK-01 | Owner, pengepak | Stok FPKA S1. Pesanan dari berkas FPKA sudah ada di Pesanan Masuk (setelah PCK-09). | 1. Owner: di /pesanan, tekan "Unggah Label Shopee", pilih berkas "Label Pengiriman.pdf", lalu "Baca label". Hasil label harus "dibuat".<br>2. Centang pesanan FPKA. Di "Tugaskan ke" pilih "Pengepak Gudang". Biarkan "Ambil dari batch" pada "Otomatis (kedaluwarsa terdekat)".<br>3. Tekan "Kirim tugas packing terpilih".<br>4. Pengepak: buka /tiket, masuk ke tiket, tekan "Mulai packing", centang barang, lalu tekan "Barang sudah disiapkan".<br>5. Owner: buka tiket di /tiket/[id], tekan "Selesaikan". | Pada langkah 1 sampai 4 stok tetap S1. Setelah "Selesaikan", stok FPKA turun 1 (menjadi S1 - 1). Status tiket "Selesai" dengan tulisan "Selesai pada … Penjualan sudah tercatat dan stok sudah dikurangi." Transaksi kanal Shopee muncul di /laporan. Kolom Pembeli berisi "Pre-order" atau nama penerima, sesuai aturan sistem. | | | | |
| PCK-02 | Owner, pengepak | Produk CBRA dengan batch CBRA-A dan CBRA-B. Berkas CBRA sudah dibaca dan pesanan ada di Pesanan Masuk. | 1. Centang pesanan CBRA. Di "Tugaskan ke" pilih "Pengepak Gudang".<br>2. Di "Ambil dari batch", pilih batch CBRA-B (kedaluwarsa paling jauh). Pilihannya tampil sebagai "exp … · rak · sisa N".<br>3. Tekan "Kirim tugas packing terpilih".<br>4. Pengepak dan owner lanjutkan sesuai PCK-06, PCK-08, dan PCK-07. Tekan "Selesaikan" di tab pertama sebagai akhir PCK-02. | Setelah "Selesaikan", sisa batch CBRA-B turun 1. Sisa CBRA-A tidak berubah. Stok total CBRA turun 1. Tiket berstatus "Selesai". | | | | |
| PCK-03 | Owner, pengepak | Stok MDMA S3. Berkas MDMA sudah dibaca dan dikirim ke pengepak. | 1. Owner: unggah dan baca berkas "Label Pengiriman(3).pdf". Centang pesanan MDMA, pilih "Tugaskan ke" pengepak, lalu "Kirim tugas packing terpilih".<br>2. Pengepak: buka tiket, tekan "Mulai packing".<br>3. Owner: buka tiket di /tiket/[id], tekan "Batalkan tugas". | Pesanan kembali ke daftar Pesanan Masuk di /pesanan dengan keterangan "Stok tidak berubah." Stok MDMA tetap S3. | | | | |
| PCK-04 | Owner | Pesanan MDMA dari PCK-03 ada di Pesanan Masuk dan belum punya tiket. | 1. Di /pesanan, centang pesanan MDMA.<br>2. Tekan "Batalkan terpilih". | Pesanan hilang dari daftar. Tidak ada tiket baru. Stok MDMA tetap S3. | | | | |
| PCK-05 | Owner | Berkas FPKA sudah dicatat di PCK-01. | 1. Di /pesanan, tekan "Unggah Label Shopee", pilih ulang "Label Pengiriman.pdf", lalu "Baca label". | Muncul pesan `Pesanan ini sudah pernah dicatat.` Tidak ada pesanan atau tiket ganda. Stok FPKA tetap. | | | | |
| PCK-06 | Pengepak | Tiket CBRA berstatus "Sedang dipacking". Belum semua barang dicentang. | 1. Pengepak: buka tiket CBRA, pastikan sudah "Mulai packing".<br>2. Jangan centang barang.<br>3. Tekan "Barang sudah disiapkan". | Muncul pesan `Masih ada barang yang belum dicentang sebagai sudah dikemas.` Status tiket tetap "Sedang dipacking". Stok CBRA tetap. | | | | |
| PCK-07 | Owner (dua tab) | Tiket CBRA berstatus "Menunggu ACC owner" (sudah "Barang sudah disiapkan"). Dua tab /tiket/[id] owner terbuka. | 1. Di tab 1, tekan "Selesaikan".<br>2. Di tab 2 (yang dibuka sebelum tab 1 selesai), tekan "Selesaikan".<br>3. Muat ulang tab 2. | Tab 1 berhasil (ini adalah hasil PCK-02). Tab 2 menampilkan `Tugas packing berstatus "handed_over" tidak bisa dipindahkan ke "handed_over".` Stok CBRA berkurang satu kali saja. | | | | |
| PCK-08 | Pengepak | Tiket CBRA berstatus "Menunggu ACC owner". | 1. Login sebagai pengepak.<br>2. Buka /tiket/[id] untuk tiket CBRA. | Tidak ada tombol "Selesaikan". Hanya tombol "Kirim ke WhatsApp" yang tampil. Stok tidak berubah. | | | | |
| PCK-09 | Owner dan pengepak | Stok FPKA S1. Pesanan FPKA sudah dikirim ke pengepak. | 1. Pengepak: buka tiket FPKA, tekan "Mulai packing", centang barang, lalu tekan "Barang sudah disiapkan". Status menjadi "Menunggu ACC owner".<br>2. Owner: di /produk/[id] produk FPKA, bagian "Koreksi stok", isi "Selisih" dengan -S1, pilih alasan "Salah catat (bukan kerugian)", lalu tekan "Koreksi stok". Stok FPKA menjadi 0.<br>3. Owner: di /tiket/[id], tekan "Selesaikan".<br>4. Owner: di /tiket/[id], tekan "Batalkan tugas".<br>5. Owner: di /produk/[id] produk FPKA, "Catat batch" dengan nomor "FPKA-UAT-R" dan jumlah S1. | Langkah 3: muncul pesan galat stok atau batch, misalnya `Stok "<nama produk FPKA>" tinggal 0, tidak cukup untuk 1.` atau `Batch "<nomor batch>" untuk "<nama produk FPKA>" tinggal 0, tidak cukup untuk 1.` Tiket tetap "Menunggu ACC owner", stok tetap 0.<br>Langkah 4: pesanan kembali ke Pesanan Masuk dengan keterangan "Stok tidak berubah." Stok tetap 0.<br>Langkah 5: stok FPKA kembali menjadi S1. Pesanan siap dikirim ulang di PCK-01. | | | | |
| PCK-10 | Owner | PCK-01 selesai. Transaksi Shopee dari berkas FPKA ada di /laporan. | 1. Buka /laporan, lalu klik nomor pesanan transaksi FPKA.<br>2. Isi alasan "uji void packing", tekan "Batalkan Transaksi", lalu setujui dialog.<br>3. Cek status tiket FPKA di /tiket/[id]. | Lencana "Dibatalkan" tampil. Stok FPKA naik 1 kembali ke batch asalnya. Tiket **kembali ke antrean** (belum ditugaskan, centang packing kosong, catatan memuat "Penjualan dibatalkan (void)") dan pesanannya berstatus "Diproses" lagi. Pilihan batch tiket tetap. Menyelesaikan tiket lagi membuat transaksi baru dan stok FPKA turun 1 lagi. | | | | | Sudah diperbaiki: void tidak lagi meninggalkan tiket "Selesai" tanpa penjualan. |
| PCK-12 | Owner | Pesanan Shopee berstatus "Dibatalkan" di platform (setelah sinkron) dan belum punya tiket, atau tiketnya belum selesai. | 1. Coba buat tugas packing untuk pesanan itu, atau tekan "Selesaikan" pada tiketnya. | Ditolak dengan pesan `Pesanan ini sudah dibatalkan oleh platform, jadi tidak bisa diproses sebagai tugas packing.` Stok tetap dan tidak ada transaksi baru. | | | | | Hanya bisa diuji bila ada pesanan berstatus dibatalkan; di lokal bisa diatur developer lewat SQL (uji API sudah lulus). |
| PCK-11 | Owner | Hanya bisa dijalankan bila ada label bervarian. Tidak ada di folder uji. | Ikuti PCK-01 dengan label bervarian. | Stok berkurang pada varian yang cocok dengan teks Variasi di label, bukan pada induk. | | | | Belum dijalankan. Ketiga label tidak bervarian. Skenario ini menunggu label bervarian. |

### 5.6 Edit massal atribut stok (sumber F)

Semua skenario di bagian ini dilakukan owner di /produk/edit-massal. Alurnya: "Unduh xlsx atribut stok", ubah kolom `sisa stok` pada baris batch, pilih kelompok "Atribut stok", tekan "Unggah dan lihat pratinjau", lalu tekan "Terapkan N baris" bila pratinjau benar.

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| MAS-01 | Owner | Batch CBSB-2508 punya sisa 24 (atau angka saat itu). | 1. Unduh xlsx atribut stok.<br>2. Ubah `sisa stok` batch CBSB-2508 dari 24 menjadi 20 dan isi kolom `alasan pengurangan` dengan `rusak`.<br>3. Pilih kelompok "Atribut stok", lalu "Unggah dan lihat pratinjau".<br>4. Di pratinjau, baris menampilkan label "Edit" dengan angka lama ke baru.<br>5. Tekan "Terapkan 1 baris". | Stok CBSB turun 4. Di /laporan/stok, "Pengurangan Stok Manual" mencatat Alasan "Rusak". | | | | | Sudah diperbaiki: pengurangan lewat edit massal wajib beralasan. |
| MAS-02 | Owner | Sisa CBSB-2508 sudah 20 (hasil MAS-01). | 1. Unduh berkas baru.<br>2. Ubah `sisa stok` CBSB-2508 menjadi 22 (masih di bawah isi kiriman 24).<br>3. Unggah, lihat pratinjau, lalu tekan "Terapkan 1 baris". | Pratinjau menampilkan "Edit". Stok CBSB naik 2. | | | | |
| MAS-03 | Owner | Isi kiriman CBSB-2508 adalah 24. | 1. Unduh berkas baru.<br>2. Ubah `sisa stok` CBSB-2508 menjadi 25.<br>3. Unggah dan lihat pratinjau. | Baris ditandai "Error" dengan pesan `sisa stok: harus antara 0 dan isi kiriman (24).` Tombol "Terapkan" tidak menerapkan baris ini. Stok tetap. | | | | |
| MAS-04 | Owner | MAS-02 sudah diterapkan. Pratinjau masih terbuka. | 1. Pada pratinjau yang sama, tekan "Terapkan 1 baris" untuk kedua kalinya. | Muncul pesan `Pratinjau ini sudah diterapkan.` Stok tidak berubah lagi. | | | | |
| MAS-05 | Owner dan kasir | Stok CBSB S. Owner sudah mengunduh berkas atribut stok (sebelum penjualan). | 1. Owner: unduh berkas atribut stok dan simpan.<br>2. Kasir: jual 2 CBSB di /kasir (selesaikan transaksi).<br>3. Owner: unggah berkas yang diunduh tanpa perubahan, lalu lihat pratinjau.<br>4. Pada pratinjau, lanjut ke langkah 4 di kolom hasil. | Pratinjau menampilkan label "Edit" dengan angka lama (sebelum penjualan). 4. Tekan "Terapkan". Halaman hasil menampilkan "0 berhasil, 1 gagal" dengan pesan `Sisa batch sudah berubah sejak pratinjau (dulu X, sekarang Y).` Stok tetap S dikurangi 2 (tidak tertimpa). Unduh ulang berkas lalu unggah lagi untuk melanjutkan. | | | | | Sudah diperbaiki: penjualan di antara pratinjau dan Terapkan tidak lagi tertimpa. |

### 5.7 Lain-lain (sumber H dan pemeriksaan akhir)

| ID | Peran | Prakondisi | Langkah | Hasil yang diharapkan | Stok sebelum | Stok sesudah | Lulus/Gagal | Catatan |
|---|---|---|---|---|---|---|---|---|
| PRD-01 | Owner, kasir | Login sebagai owner. Buat produk uji baru khusus skenario ini dengan SKU uji (misalnya "UAT-HAPUS") dan "Stok awal" 3. | 1. Kasir: jual 1 produk uji di /kasir (selesaikan transaksi).<br>2. Owner: buka /produk/[id] produk uji, lalu bagian "Hapus".<br>3. Tekan "Hapus produk" dan ikuti konfirmasi bila muncul.<br>4. Cek /produk dan /laporan. | Penghapusan **ditolak** dengan pesan "Nonaktifkan saja" karena produk sudah ada di transaksi kasir. Produk tetap ada di /produk dan transaksi tetap ada di /laporan. Lalu buat produk uji kedua tanpa penjualan, batalkan batch-nya, dan hapus: produk hilang dari /produk. | | | | | Perilaku baru: produk yang punya jejak transaksi, stok lebih dari 0, tugas packing, atau pesanan marketplace tidak bisa dihapus. |
| RIW-01 | Owner | Login sebagai owner. Ada produk dengan penjualan dan koreksi (misalnya CBSB). | 1. Buka /produk/[id] produk itu, bagian "Batch & Stok".<br>2. Tekan "Lihat riwayat perubahan stok".<br>3. Login sebagai kasir, buka /produk/[id]/riwayat. | Langkah 2 menampilkan tabel perubahan stok terbaru dulu: waktu, sebab (Penjualan, Koreksi manual, Void transaksi, Pesanan marketplace, Batch masuk, Batch dibatalkan), perubahan bertanda + atau -, stok sebelum, stok sesudah, dan alasan koreksi. "Stok sesudah" baris teratas sama dengan stok produk.<br>Langkah 3 ditolak: kasir tidak punya akses. | | | | Maksimal 100 perubahan terakhir. |
| AKH-01 | Owner | Semua skenario di lingkungan lokal sudah dijalankan. Untuk staging, lakukan hal yang sama untuk FPKA, CBRA, dan MDMA. | 1. Untuk tiap SKU uji (CBSB, HJA, AU08, FDU), buka /produk lalu /produk/[id]. Bandingkan kolom Stok di /produk dengan jumlah kolom Sisa di /produk/[id].<br>2. Buka /laporan/stok. Cek bahwa "Nilai Persediaan Sekarang" sesuai dengan nilai dari sisa batch.<br>3. Minta developer menjalankan SQL kedua di Lampiran a. | Stok di /produk sama dengan jumlah sisa batch untuk setiap SKU. Nilai Persediaan Sekarang konsisten. SQL kedua menghasilkan 0 baris. | | | | |

---

## 6. Pemeriksaan akhir

Setelah semua skenario selesai, penguji memeriksa hal berikut:

1. Stok total setiap SKU uji sama dengan jumlah sisa batchnya (AKH-01).
2. SQL kedua di Lampiran a menghasilkan 0 baris. Bila tidak, catat sebagai temuan dan jangan tutup UAT.
3. Ketiga kejanggalan awal (a, b, c) sudah diperbaiki, jadi bila salah satunya muncul lagi dihitung "Gagal".
4. Ringkasan di Bagian 9 sudah terisi.

---

## 7. Di luar cakupan

- **Sinkronisasi API Shopee dan TikTok.** Tidak diuji di sini. API tidak punya tampilan yang bisa diuji, dan API tidak mengubah stok secara langsung.
- **Notifikasi push.** Tidak diuji di sini. Notifikasi tidak mengubah stok.
- **Pengujian beban dan kinerja.** Tidak termasuk.
- **Pengujian dengan label packing yang bervarian atau qty lebih dari 1.** Belum tersedia di folder uji (lihat Bagian 3.2). PCK-11 ditandai belum dijalankan.

---

## 8. Lampiran

### a. Verifikasi teknis (SQL)

Dijalankan developer dengan perintah:

```bash
docker compose exec postgres psql -U postgres -d aj33
```

Kueri pertama, untuk melihat 10 mutasi terakhir pada produk CBSB:

```sql
SELECT sa.created_at, sa.change_qty, sa.reason, sa.manual_reason, sa.stock_before, sa.stock_after, b.batch_number
FROM stock_adjustments sa LEFT JOIN product_batches b ON b.id = sa.batch_id
WHERE sa.product_id = (SELECT id FROM products WHERE sku = 'CBSB-AFC-2KG')
ORDER BY sa.created_at DESC LIMIT 10;
```

Kueri kedua, untuk memeriksa bahwa stok total sama dengan jumlah sisa batch. Hasilnya harus **0 baris**:

```sql
SELECT p.sku, p.stock_qty, COALESCE(SUM(b.remaining_qty),0) AS sisa_batch
FROM products p LEFT JOIN product_batches b ON b.product_id = p.id
GROUP BY p.id HAVING p.stock_qty <> COALESCE(SUM(b.remaining_qty),0);
```

Nilai kolom `reason` yang diharapkan:

| Sumber | Nilai `reason` |
|---|---|
| Penjualan kasir dan tiket packing | `sale` |
| Void | `void_reversal` |
| Koreksi stok dan edit massal | `manual_adjustment` |
| Catat batch | `restock` |
| Batalkan batch | `restock` dengan nilai negatif |

Kolom verifikasi per skenario (diisi developer):

| ID | Yang diperiksa | Hasil SQL (diisi developer) | Verifikasi teknis |
|---|---|---|---|
| KSR-01 | Ada satu baris `sale` dengan nilai -2 | | |
| KSR-05 | Tidak ada baris baru setelah kirim ulang | | |
| VOID-01 | Ada baris `void_reversal` +2 pada batch CBSB-2508 | | |
| VOID-03 | Hanya satu baris `void_reversal` | | |
| KOR-06 | Baris `manual_adjustment` +3 dengan batch baru tanpa nomor | | |
| KOR-09 | Dua baris `manual_adjustment` pada dua batch AU08 | | |
| BAT-02 | Baris `restock` bernilai -5 | | |
| PCK-01 | Baris `sale` untuk batch FPKA yang dipilih otomatis | | |
| PCK-02 | Baris `sale` hanya pada CBRA-B | | |
| PCK-07 | Hanya satu baris `sale` untuk tiket CBRA | | |
| PCK-09 | Tidak ada baris `sale` saat "Selesaikan" gagal | | |
| MAS-01 | Baris `manual_adjustment` -4 | | |
| AKH-01 | Kueri kedua menghasilkan 0 baris | | |

### b. Glosarium

- **Batch:** kelompok barang dari satu kiriman atau satu catatan masuk. Setiap batch punya nomor, sisa, dan tanggal kedaluwarsa sendiri. Stok produk adalah jumlah sisa semua batchnya.
- **FEFO:** singkatan dari "first expired, first out". Batch dengan kedaluwarsa paling dekat keluar lebih dulu.
- **Void:** pembatalan transaksi. Stok barang dikembalikan ke batch asalnya, dan transaksi diberi lencana "Dibatalkan".
- **Tiket:** tugas packing. Tiket dibuat dari pesanan Shopee dan diproses pengepak sampai owner menyelesaikannya.

### c. Daftar pesan galat dan pesan sukses

| Pesan | Skenario |
|---|---|
| `Stok "<nama>" tinggal N, tidak cukup untuk M.` | KOR-08, PCK-09 (bila stok total yang dicek) |
| `Batch <nomor batch> untuk "<nama>" tinggal N, tidak cukup untuk M.` | KSR-06, PCK-09 (bila batch yang dicek) |
| `Nominal pembayaran kurang dari total belanja.` | KSR-08 |
| `Keranjang masih kosong.` | Uji kasir tambahan bila diperlukan |
| `Transaksi ini sudah dibatalkan.` | VOID-03 |
| `Pilih alasan pengurangan stok: rusak, kedaluwarsa, hilang, atau salah catat.` | KOR-05 |
| `Perubahan stok tidak boleh nol.` | KOR-07 |
| `Batch "UAT-02" sudah pernah dicatat untuk produk ini.` | BAT-04 |
| `Produk ini punya varian. Catat batch pada variannya, bukan di induk.` | Catat batch pada produk induk bervarian |
| `"X" tidak bisa dihapus karena … Nonaktifkan saja.` | Hapus produk yang masih punya keterkaitan |
| `sisa stok: harus antara 0 dan isi kiriman (N).` | MAS-03 |
| `Pratinjau ini sudah diterapkan.` | MAS-04 |
| `Pesanan ini sudah pernah dicatat.` | PCK-05 |
| `SKU X tidak ada di produk aktif. Samakan SKU di Shopee dengan SKU di app.` | Label dengan SKU yang tidak ada di produk aktif |
| `Masih ada barang yang belum dicentang sebagai sudah dikemas.` | PCK-06 |
| `Hanya owner yang bisa menyelesaikan tugas packing, setelah ACC di WhatsApp.` | Selesaikan oleh selain owner |
| `Tugas packing berstatus "handed_over" tidak bisa dipindahkan ke "handed_over".` | PCK-07 |
| `Yakin ingin membatalkan transaksi ini? Stok akan dikembalikan otomatis…` | VOID-01 (dialog konfirmasi) |
| `Stok dikoreksi.` | KOR-01 sampai KOR-06 (sukses) |
| `Batch dibatalkan dan stoknya ditarik kembali.` | BAT-02 (sukses) |
| `Selesai pada … Penjualan sudah tercatat dan stok sudah dikurangi.` | PCK-01 (sukses) |

### d. Glosarium tambahan untuk istilah laporan

- **Pengurangan Stok Manual:** tabel di /laporan/stok berisi koreksi stok turun dan edit massal turun. Kolomnya Waktu, Produk, Jumlah, Alasan, Nilai, dan Oleh.
- **Kerugian Stok:** kartu di /laporan/stok yang hanya menghitung alasan Rusak, Kedaluwarsa, dan Hilang (selisih opname). Alasan "Salah catat (bukan kerugian)" dan "Tanpa alasan" tidak dihitung.

---

## 9. Ringkasan hasil, temuan, dan pengesahan

### 9.1 Ringkasan hasil

| Kelompok | Jumlah skenario | Lulus | Gagal | Dicatat saja |
|---|---|---|---|---|
| Penjualan kasir (KSR) | 10 | | | |
| Void (VOID) | 5 | | | |
| Koreksi manual (KOR) | 10 | | | |
| Batch (BAT) | 4 | | | |
| Tugas packing (PCK) | 11 | | | |
| Edit massal (MAS) | 5 | | | |
| Lain-lain (PRD, AKH) | 2 | | | |
| **Total** | **47** | | | |

### 9.2 Daftar temuan

| No | ID skenario | Deskripsi temuan | Tingkat (Kritis, Tinggi, Sedang, Rendah) | Tindak lanjut |
|---|---|---|---|---|
| 1 | | | | |
| 2 | | | | |
| 3 | | | | |

### 9.3 Lembar pengesahan

| Peran | Nama | Tanggal | Tanda tangan |
|---|---|---|---|
| Penguji | | | |
| Owner | | | |
| Developer | | | |
