/**
 * Laporan keuangan harian: menarik transaksi dari aplikasi dan mengisi satu tab per tanggal dengan layout dan warna template Offline | Shopee | Tokopedia.
 * Pasang di Extensions > Apps Script pada spreadsheet, isi Project Settings > Script properties (URL_APP, TOKEN_SHEET), lalu jalankan pasangPemicu() sekali.
 */

const MAKS_ITEM = 5;
const ZONA = 'Asia/Jakarta';
const AWAL = 6;

// Nomor kolom 1-based mengikuti template; kapasitas = jumlah baris kosong bawaan template, bertambah sendiri bila transaksi lebih banyak.
// `bersih` null berarti blok tak punya kolom Bersih (offline) dan profit dihitung dari Cash.
const BLOK = [
  { kanal: 'toko', judul: 'Total Offline', no: 1, nama: 2, bersih: null, cash: 3, item: 4, modal: 9, profit: 10, kapasitas: 32, warnaData: '#d9d9ea' },
  { kanal: 'shopee', judul: 'Total Shopee', no: 12, nama: 13, bersih: 14, cash: 15, item: 16, modal: 21, profit: 22, kapasitas: 44, warnaData: '#cfe2f3' },
  { kanal: 'tiktok', judul: 'Total Tokopedia', no: 24, nama: 25, bersih: 26, cash: 27, item: 28, modal: 33, profit: 34, kapasitas: 10, warnaData: '#ffe599' },
];

const WARNA = {
  biru: '#4472c4',
  hijauMuda: '#e2efda',
  hijauSedang: '#a9d18e',
  hijauTua: '#548235',
  peach: '#f9cb9c',
  ungu: '#674ea7',
  emas: '#bf9000',
  kuning: '#ffff00',
  oranye: '#f1c232',
  hijauLabel: '#93c47d',
  hijauJudul: '#38761d',
  merah: '#ea9999',
  merahTua: '#cc4125',
  tosca: '#134f5c',
};

// Blok tambahan di kanan Tokopedia: rincian potongan Shopee dan Tokopedia sebaris dengan blok kanalnya, dan daftar pengeluaran hari itu.
// `mulai` adalah kolom nama; kolom potongan menyusul sesuai urutan `kolom` (kunci field transaksi, judul), lalu kolom Total.
const POTONGAN = [
  { kanal: 'shopee', judul: 'Potongan Shopee', mulai: 36, kolom: [['komisi', 'Komisi'], ['layanan', 'Layanan'], ['pph', 'PPh 0,5%'], ['biaya_proses', 'Biaya proses']] },
  { kanal: 'tiktok', judul: 'Potongan Tokopedia', mulai: 47, kolom: [['komisi', 'Komisi'], ['komisi_dinamis', 'Komisi dinamis'], ['logistik', 'Layanan logistik'], ['pph', 'PPh 0,5%'], ['biaya_proses', 'Biaya proses']] },
];
const KELUAR = { keterangan: 43, kategori: 44, jumlah: 45, kapasitas: 10 };

const FORMAT_UANG = '#,##0';
const FORMAT_UANG_STRIP = '#,##0;-#,##0;"-"';

function sinkronHariIni() {
  sinkronTanggal(Utilities.formatDate(new Date(), ZONA, 'yyyy-MM-dd'));
}

/** Tanggal `yyyy-MM-dd`; tab bernama tanggal itu dibuat bila belum ada dan ditimpa bila sudah. */
function sinkronTanggal(tanggal) {
  const prop = PropertiesService.getScriptProperties();
  const url = prop.getProperty('URL_APP') + '/sheet/harian?tanggal=' + encodeURIComponent(tanggal);
  const res = UrlFetchApp.fetch(url, {
    headers: { Authorization: 'Bearer ' + prop.getProperty('TOKEN_SHEET') },
    muteHttpExceptions: true,
  });
  if (res.getResponseCode() !== 200) {
    throw new Error('Aplikasi menjawab ' + res.getResponseCode() + ': ' + res.getContentText());
  }
  const data = JSON.parse(res.getContentText());

  const ss = SpreadsheetApp.getActiveSpreadsheet();
  const sheet = ss.getSheetByName(tanggal) || ss.insertSheet(tanggal, 0);
  sheet.clear();

  BLOK.forEach(function (b) {
    tulisBlok(sheet, b, data.transaksi.filter(function (t) { return t.kanal === b.kanal; }));
  });
  POTONGAN.forEach(function (p) {
    tulisPotongan(sheet, p, data.transaksi.filter(function (t) { return t.kanal === p.kanal; }));
  });
  tulisPengeluaran(sheet, data);
  tulisRingkasan(sheet, data.transaksi);
  aturLebarKolom(sheet);
}

function huruf(kol) {
  let s = '';
  while (kol > 0) {
    const m = (kol - 1) % 26;
    s = String.fromCharCode(65 + m) + s;
    kol = Math.floor((kol - 1) / 26);
  }
  return s;
}

function warnai(sheet, a1, latar, opsi) {
  const r = sheet.getRange(a1);
  if (latar) r.setBackground(latar);
  opsi = opsi || {};
  if (opsi.teks) r.setFontColor(opsi.teks);
  if (opsi.tebal) r.setFontWeight('bold');
  if (opsi.ukuran) r.setFontSize(opsi.ukuran);
  if (opsi.tengah) r.setHorizontalAlignment('center');
  return r;
}

function garis(range) {
  range.setBorder(true, true, true, true, true, true, '#000000', SpreadsheetApp.BorderStyle.SOLID);
}

function tulisBlok(sheet, b, transaksi) {
  const jumlah = Math.max(b.kapasitas, transaksi.length);
  const akhir = AWAL + jumlah - 1;
  const lebar = b.profit - b.no + 1;
  const kolItemAkhir = b.item + MAKS_ITEM - 1;
  const dasar = b.bersih || b.cash;

  const baris = [];
  for (let i = 0; i < jumlah; i++) {
    const r = AWAL + i;
    const sel = new Array(lebar).fill('');
    const t = transaksi[i];
    sel[0] = i + 1;
    if (t) {
      const item = new Array(MAKS_ITEM).fill(0);
      // lebih dari 5 produk: sisanya dijumlahkan ke Item 5 agar modal total tetap benar
      t.modal.forEach(function (m, n) { item[Math.min(n, MAKS_ITEM - 1)] += Number(m); });
      sel[b.nama - b.no] = t.pelanggan || '';
      if (b.bersih) sel[b.bersih - b.no] = Number(t.bersih);
      sel[b.cash - b.no] = Number(t.bruto);
      item.forEach(function (v, n) { sel[b.item - b.no + n] = v || ''; });
    }
    sel[b.modal - b.no] = '=SUM(' + huruf(b.item) + r + ':' + huruf(kolItemAkhir) + r + ')';
    // di Shopee PPh sudah terpotong dari Bersih; di blok lain PPh adalah beban toko dan ikut mengurangi profit, seperti di aplikasi
    const pph = !b.bersih && t ? Number(t.pph) : 0;
    sel[b.profit - b.no] = '=' + huruf(dasar) + r + '-' + huruf(b.modal) + r + (pph ? '-' + pph : '');
    baris.push(sel);
  }
  sheet.getRange(AWAL, b.no, jumlah, lebar).setValues(baris);

  sheet.getRange(5, b.nama).setValue('Customer');
  if (b.bersih) sheet.getRange(5, b.bersih).setValue('Bersih');
  sheet.getRange(5, b.cash).setValue('Cash');
  for (let n = 1; n <= MAKS_ITEM; n++) sheet.getRange(5, b.item + n - 1).setValue('Item ' + n);

  const rentang = function (kol) { return huruf(kol) + AWAL + ':' + huruf(kol) + akhir; };
  sheet.getRange(4, b.nama).setValue(b.judul);
  if (b.bersih) sheet.getRange(4, b.bersih).setFormula('=SUM(' + rentang(b.bersih) + ')');
  sheet.getRange(4, b.cash).setFormula('=SUM(' + rentang(b.cash) + ')');
  sheet.getRange(4, b.modal).setFormula('=SUM(' + rentang(b.modal) + ')');
  sheet.getRange(4, b.profit).setFormula('=SUM(' + rentang(b.profit) + ')');

  // warna: judul dan total biru, header peach, data per kolom sesuai template
  const awalTotal = b.nama;
  const akhirTotal = b.cash;
  warnai(sheet, huruf(awalTotal) + '4:' + huruf(akhirTotal) + '4', WARNA.biru, { teks: '#ffffff', tebal: true, ukuran: 12 });
  warnai(sheet, huruf(b.modal) + '4', WARNA.hijauSedang, { tebal: true });
  warnai(sheet, huruf(b.profit) + '4', WARNA.hijauTua, { teks: '#ffffff', tebal: true });
  warnai(sheet, huruf(b.nama) + '5:' + huruf(kolItemAkhir) + '5', WARNA.peach, { tengah: true });

  warnai(sheet, huruf(b.nama) + AWAL + ':' + huruf(b.cash) + akhir, b.warnaData);
  warnai(sheet, huruf(b.item) + AWAL + ':' + huruf(kolItemAkhir) + akhir, WARNA.hijauMuda);
  warnai(sheet, rentang(b.modal), WARNA.hijauSedang);
  warnai(sheet, rentang(b.profit), WARNA.hijauTua, { teks: WARNA.kuning, tebal: true });

  garis(sheet.getRange(5, b.nama, jumlah + 1, kolItemAkhir - b.nama + 1));
  garis(sheet.getRange(AWAL, b.modal, jumlah, 2));
  garis(sheet.getRange(AWAL, b.no, jumlah, 1));
  sheet.getRange(AWAL, b.no, jumlah, 1).setHorizontalAlignment('center');

  sheet.getRange(AWAL, b.cash, jumlah, kolItemAkhir - b.cash + 1).setNumberFormat(FORMAT_UANG);
  if (b.bersih) sheet.getRange(AWAL, b.bersih, jumlah, 1).setNumberFormat(FORMAT_UANG);
  sheet.getRange(AWAL, b.modal, jumlah, 2).setNumberFormat(FORMAT_UANG_STRIP);
  sheet.getRange(4, awalTotal, 1, b.profit - awalTotal + 1).setNumberFormat(FORMAT_UANG_STRIP);

  // modal belum lengkap: nama dan modal merah supaya profit baris itu tak dipercaya begitu saja
  transaksi.forEach(function (t, i) {
    if (!t.modal_tidak_lengkap) return;
    const r = AWAL + i;
    sheet.getRange(r, b.nama).setBackground(WARNA.merah)
      .setNote('Modal belum lengkap: ada barang tanpa harga beli, jadi profit baris ini terlalu tinggi.');
    sheet.getRange(r, b.modal).setBackground(WARNA.merah);
  });
}

/** Rincian potongan satu marketplace sebaris dengan blok kanalnya; kapasitas baris mengikuti blok kanal itu. */
function tulisPotongan(sheet, p, transaksi) {
  const kapasitas = BLOK.filter(function (b) { return b.kanal === p.kanal; })[0].kapasitas;
  const jumlah = Math.max(kapasitas, transaksi.length);
  const akhir = AWAL + jumlah - 1;
  const awalFee = p.mulai + 1;
  const akhirFee = p.mulai + p.kolom.length;
  const kolTotal = akhirFee + 1;
  const lebar = p.kolom.length + 2;

  const baris = [];
  for (let i = 0; i < jumlah; i++) {
    const r = AWAL + i;
    const t = transaksi[i];
    const sel = [t ? t.pelanggan || '' : ''];
    p.kolom.forEach(function (k) { sel.push(t ? Number(t[k[0]]) : ''); });
    sel.push('=SUM(' + huruf(awalFee) + r + ':' + huruf(akhirFee) + r + ')');
    baris.push(sel);
  }
  sheet.getRange(AWAL, p.mulai, jumlah, lebar).setValues(baris);

  const judul = [p.judul].concat(p.kolom.map(function (k) { return k[1]; }), ['Total']);
  sheet.getRange(5, p.mulai, 1, lebar).setValues([judul]);
  warnai(sheet, huruf(p.mulai) + '5:' + huruf(kolTotal) + '5', WARNA.peach, { tengah: true, tebal: true });
  sheet.getRange(4, p.mulai).setValue('Total potongan');
  for (let kol = awalFee; kol <= kolTotal; kol++) {
    sheet.getRange(4, kol).setFormula('=SUM(' + huruf(kol) + AWAL + ':' + huruf(kol) + akhir + ')');
  }
  warnai(sheet, huruf(p.mulai) + '4:' + huruf(kolTotal) + '4', WARNA.biru, { teks: '#ffffff', tebal: true });
  warnai(sheet, huruf(p.mulai) + AWAL + ':' + huruf(akhirFee) + akhir, WARNA.hijauMuda);
  warnai(sheet, huruf(kolTotal) + AWAL + ':' + huruf(kolTotal) + akhir, WARNA.hijauSedang, { tebal: true });
  garis(sheet.getRange(5, p.mulai, jumlah + 1, lebar));
  sheet.getRange(4, awalFee, jumlah + 2, p.kolom.length + 1).setNumberFormat(FORMAT_UANG_STRIP);
}

function tulisPengeluaran(sheet, data) {
  const K = KELUAR;
  const baris = data.pengeluaran.map(function (e) {
    return [e.keterangan || '', e.kategori || '', Number(e.jumlah)];
  });
  // ongkir yang ditanggung toko adalah beban juga di aplikasi, tapi tersimpan di transaksi, bukan di tabel pengeluaran
  const subsidi = data.transaksi.reduce(function (s, t) { return s + Number(t.subsidi_ongkir); }, 0);
  if (subsidi > 0) baris.push(['Subsidi ongkir (otomatis dari transaksi)', 'ongkir', subsidi]);

  const jumlah = Math.max(K.kapasitas, baris.length);
  const akhir = AWAL + jumlah - 1;
  while (baris.length < jumlah) baris.push(['', '', '']);
  sheet.getRange(AWAL, K.keterangan, jumlah, 3).setValues(baris);

  sheet.getRange(5, K.keterangan, 1, 3).setValues([['Keterangan', 'Kategori', 'Jumlah']]);
  warnai(sheet, huruf(K.keterangan) + '5:' + huruf(K.jumlah) + '5', WARNA.peach, { tengah: true, tebal: true });
  sheet.getRange(4, K.keterangan).setValue('Total Pengeluaran');
  sheet.getRange(4, K.jumlah).setFormula('=SUM(' + huruf(K.jumlah) + AWAL + ':' + huruf(K.jumlah) + akhir + ')');
  warnai(sheet, huruf(K.keterangan) + '4:' + huruf(K.jumlah) + '4', WARNA.merahTua, { teks: '#ffffff', tebal: true });
  warnai(sheet, huruf(K.keterangan) + AWAL + ':' + huruf(K.jumlah) + akhir, '#f4cccc');
  garis(sheet.getRange(5, K.keterangan, jumlah + 1, 3));
  sheet.getRange(4, K.jumlah, jumlah + 2, 1).setNumberFormat(FORMAT_UANG_STRIP);
}

function tulisRingkasan(sheet, transaksi) {
  const toko = BLOK[0];
  const shopee = BLOK[1];
  const tiktok = BLOK[2];
  const pph = function (kanal) {
    return transaksi.filter(function (t) { return t.kanal === kanal; })
      .reduce(function (s, t) { return s + Number(t.pph); }, 0);
  };
  const sel = function (kol) { return huruf(kol) + '4'; };
  // omzet = uang offline + uang yang cair dari marketplace (kolom Bersih)
  const omzet = '=' + sel(toko.cash) + '+' + sel(shopee.bersih) + '+' + sel(tiktok.bersih);
  const modal = '=' + sel(toko.modal) + '+' + sel(shopee.modal) + '+' + sel(tiktok.modal);
  const profit = '=' + sel(toko.profit) + '+' + sel(shopee.profit) + '+' + sel(tiktok.profit);
  const omzetBruto = '=' + sel(toko.cash) + '+' + sel(shopee.cash) + '+' + sel(tiktok.cash);

  sheet.getRange('G1').setValue('Total Omzet');
  sheet.getRange('H1').setFormula(omzet);
  sheet.getRange('G2').setValue('Modal');
  sheet.getRange('H2').setFormula(modal);
  sheet.getRange('J1').setValue('Total Profit');
  sheet.getRange('M1').setFormula(profit);
  sheet.getRange('J2').setValue('Prosentase');
  sheet.getRange('M2').setFormula('=IFERROR(M1/H1,0)');
  sheet.getRange('O1').setValue('Omzet Marketplace  & Offline');
  sheet.getRange('R1').setFormula(omzetBruto);

  [[toko, 'C', 'toko'], [shopee, 'O', 'shopee'], [tiktok, 'AA', 'tiktok']].forEach(function (p) {
    sheet.getRange(p[1] + '2').setValue('Pajak 0,5%');
    sheet.getRange(p[1] + '3').setValue(pph(p[2]));
    warnai(sheet, p[1] + '2:' + p[1] + '3', WARNA.ungu, { teks: '#ffffff', tengah: true });
    sheet.getRange(p[1] + '3').setNumberFormat(FORMAT_UANG_STRIP);
  });

  warnai(sheet, 'G1', WARNA.hijauJudul, { teks: '#ffffff', tebal: true, ukuran: 14, tengah: true });
  warnai(sheet, 'H1', WARNA.hijauTua, { teks: '#ffffff', tebal: true, ukuran: 14, tengah: true });
  warnai(sheet, 'G2:H2', WARNA.hijauLabel, { tengah: true });
  warnai(sheet, 'J1', WARNA.oranye, { tebal: true, ukuran: 14, tengah: true });
  warnai(sheet, 'M1', WARNA.oranye, { tebal: true, ukuran: 14, tengah: true });
  warnai(sheet, 'J2:M2', WARNA.kuning, { tengah: true });
  warnai(sheet, 'O1:Q1', WARNA.emas, { teks: '#ffffff', tebal: true, ukuran: 14 });
  warnai(sheet, 'R1', WARNA.emas, { teks: '#ffffff', tebal: true, ukuran: 14, tengah: true });

  // Total Profit sudah memuat PPh, jadi laba seperti di aplikasi tinggal dikurangi pengeluaran
  sheet.getRange('T1').setValue('Total Pengeluaran');
  sheet.getRange('V1').setFormula('=' + huruf(KELUAR.jumlah) + '4');
  sheet.getRange('T2').setValue('Profit Bersih');
  sheet.getRange('V2').setFormula('=M1-V1');
  warnai(sheet, 'T1:V1', WARNA.merahTua, { teks: '#ffffff', tebal: true });
  warnai(sheet, 'T2:V2', WARNA.tosca, { teks: '#ffffff', tebal: true, ukuran: 14 });
  sheet.getRange('V1:V2').setNumberFormat(FORMAT_UANG);
  sheet.getRange('T1:V2').setBorder(true, true, true, true, null, null, '#000000', SpreadsheetApp.BorderStyle.SOLID);

  sheet.getRangeList(['H1:H2', 'M1', 'R1']).setNumberFormat(FORMAT_UANG);
  sheet.getRange('M2').setNumberFormat('0%');
  sheet.getRangeList(['G1:H2', 'J1:M2', 'O1:R1']).setBorder(true, true, true, true, null, null, '#000000', SpreadsheetApp.BorderStyle.SOLID);
}

function aturLebarKolom(sheet) {
  BLOK.forEach(function (b) {
    sheet.setColumnWidth(b.no, 32);
    sheet.setColumnWidth(b.nama, 110);
    if (b.bersih) sheet.setColumnWidth(b.bersih, 90);
    sheet.setColumnWidth(b.cash, 90);
    for (let n = 0; n < MAKS_ITEM; n++) sheet.setColumnWidth(b.item + n, 80);
    sheet.setColumnWidth(b.modal, 90);
    sheet.setColumnWidth(b.profit, 90);
  });
  POTONGAN.forEach(function (p) {
    sheet.setColumnWidth(p.mulai, 110);
    for (let kol = p.mulai + 1; kol <= p.mulai + p.kolom.length + 1; kol++) sheet.setColumnWidth(kol, 85);
  });
  sheet.setColumnWidth(KELUAR.keterangan, 220);
  sheet.setColumnWidth(KELUAR.kategori, 100);
  sheet.setColumnWidth(KELUAR.jumlah, 90);
}

/** Dijalankan sekali: menyegarkan tiap 5 menit. */
function pasangPemicu() {
  ScriptApp.getProjectTriggers().forEach(function (t) { ScriptApp.deleteTrigger(t); });
  ScriptApp.newTrigger('sinkronHariIni').timeBased().everyMinutes(5).create();
}
