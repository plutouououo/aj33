/**
 * Laporan keuangan harian: menarik transaksi dari aplikasi dan mengisi satu tab per tanggal dengan layout Offline | Shopee | Tokopedia.
 * Pasang di Extensions > Apps Script pada spreadsheet, isi Project Settings > Script properties (URL_APP, TOKEN_SHEET), lalu jalankan pasangPemicu() sekali.
 */

const MAKS_ITEM = 5;
const ZONA = 'Asia/Jakarta';

// kanal di aplikasi -> blok di sheet; kolom `bersih` hanya dipakai Shopee karena di sana uang cair berbeda dari omzet
const BLOK = [
  { kanal: 'toko', judul: 'Total Offline', kolom: 2, pakaiBersih: false },
  { kanal: 'shopee', judul: 'Total Shopee', kolom: 13, pakaiBersih: true },
  { kanal: 'tiktok', judul: 'Total Tokopedia', kolom: 25, pakaiBersih: false },
];

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

  const barisBlok = BLOK.map(function (b) {
    return tulisBlok(sheet, b, data.transaksi.filter(function (t) { return t.kanal === b.kanal; }));
  });
  tulisRingkasan(sheet, barisBlok, data.transaksi);
}

/** Mengembalikan alamat sel total omzet dan profit blok supaya ringkasan merujuknya lewat rumus. */
function tulisBlok(sheet, blok, transaksi) {
  const k = blok.kolom;
  const kolomCash = k + 1;
  const kolomItem = k + 2;
  const kolomModal = kolomItem + MAKS_ITEM;
  const kolomProfit = kolomModal + 1;
  const kolomBersih = blok.pakaiBersih ? kolomModal + 2 : null;
  const awal = 6;
  const akhir = Math.max(awal + transaksi.length - 1, awal);
  const huruf = function (kol) { return sheet.getRange(1, kol).getA1Notation().replace(/\d+/, ''); };

  const header = ['Customer', 'Cash'];
  for (let i = 1; i <= MAKS_ITEM; i++) header.push('Item ' + i);
  header.push('Modal', 'Profit');
  if (blok.pakaiBersih) header.push('Bersih');
  sheet.getRange(5, k, 1, header.length).setValues([header]).setFontWeight('bold');

  const baris = transaksi.map(function (t, i) {
    const r = awal + i;
    const item = [];
    for (let n = 0; n < MAKS_ITEM; n++) item.push(Number(t.modal[n] || 0));
    // lebih dari 5 produk: sisanya dijumlahkan ke Item 5 agar modal total tetap benar
    for (let n = MAKS_ITEM; n < t.modal.length; n++) item[MAKS_ITEM - 1] += Number(t.modal[n]);
    const cash = Number(t.bruto);
    const pelanggan = t.pelanggan || '';
    const modal = '=SUM(' + huruf(kolomItem) + r + ':' + huruf(kolomItem + MAKS_ITEM - 1) + r + ')';
    const dasar = blok.pakaiBersih ? huruf(kolomBersih) + r : huruf(kolomCash) + r;
    const profit = '=' + dasar + '-' + huruf(kolomModal) + r;
    const sisi = [pelanggan, cash].concat(item, [modal, profit]);
    if (blok.pakaiBersih) sisi.push(Number(t.bersih));
    return sisi;
  });
  if (baris.length) {
    sheet.getRange(awal, k, baris.length, header.length).setValues(baris);
  }

  const rentang = function (kol) { return huruf(kol) + awal + ':' + huruf(kol) + akhir; };
  sheet.getRange(4, k).setValue(blok.judul).setFontWeight('bold');
  sheet.getRange(4, kolomCash).setFormula('=SUM(' + rentang(kolomCash) + ')');
  sheet.getRange(4, kolomModal).setFormula('=SUM(' + rentang(kolomModal) + ')');
  sheet.getRange(4, kolomProfit).setFormula('=SUM(' + rentang(kolomProfit) + ')');
  if (blok.pakaiBersih) sheet.getRange(4, kolomBersih).setFormula('=SUM(' + rentang(kolomBersih) + ')');

  const uang = sheet.getRange(awal, kolomCash, akhir - awal + 1, header.length - 1);
  uang.setNumberFormat('#,##0');
  sheet.getRange(4, kolomCash, 1, header.length - 1).setNumberFormat('#,##0');

  return {
    omzet: huruf(blok.pakaiBersih ? kolomBersih : kolomCash) + '4',
    modal: huruf(kolomModal) + '4',
    profit: huruf(kolomProfit) + '4',
  };
}

function tulisRingkasan(sheet, blok, transaksi) {
  const jumlah = function (kunci) { return '=' + blok.map(function (b) { return b[kunci]; }).join('+'); };
  const pph = function (kanal) {
    return transaksi.filter(function (t) { return t.kanal === kanal; })
      .reduce(function (s, t) { return s + Number(t.pph); }, 0);
  };
  sheet.getRange('G1:H1').setValues([['Total Omzet', '']]).setFontWeight('bold');
  sheet.getRange('H1').setFormula(jumlah('omzet'));
  sheet.getRange('J1').setValue('Total Profit').setFontWeight('bold');
  sheet.getRange('L1').setFormula(jumlah('profit'));
  sheet.getRange('G2').setValue('Modal');
  sheet.getRange('H2').setFormula(jumlah('modal'));
  sheet.getRange('J2').setValue('Prosentase');
  sheet.getRange('L2').setFormula('=IFERROR(L1/H1,0)').setNumberFormat('0%');
  sheet.getRange('B2').setValue('PPh 0,5% offline');
  sheet.getRange('B3').setValue(pph('toko'));
  sheet.getRange('M2').setValue('PPh 0,5% Shopee (sudah terpotong di Bersih)');
  sheet.getRange('M3').setValue(pph('shopee'));
  sheet.getRangeList(['H1:H2', 'L1', 'B3', 'M3']).setNumberFormat('#,##0');
}

/** Dijalankan sekali: menyegarkan tiap 5 menit. */
function pasangPemicu() {
  ScriptApp.getProjectTriggers().forEach(function (t) { ScriptApp.deleteTrigger(t); });
  ScriptApp.newTrigger('sinkronHariIni').timeBased().everyMinutes(5).create();
}
