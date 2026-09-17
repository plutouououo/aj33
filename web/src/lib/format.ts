/** Pemformatan angka dan tanggal, satu tempat supaya konsisten di semua halaman. */

const RUPIAH = new Intl.NumberFormat('id-ID', {
  style: 'currency',
  currency: 'IDR',
  maximumFractionDigits: 0,
});

export function rupiah(nilai: number): string {
  return RUPIAH.format(nilai);
}

/**
 * Zona waktu toko, sama dengan yang dipakai backend saat memotong laporan
 * per hari (lihat `backend/src/reports/mod.rs`).
 *
 * Disebut eksplisit karena halaman ini dirender di server, dan server
 * produksi berjalan pada UTC: tanpa ini setiap jam yang tampil meleset tujuh
 * jam, dan transaksi sore terlihat terjadi pada hari yang sama dengan
 * laporan "hari ini" yang sudah berpindah.
 */
const ZONA = 'Asia/Jakarta';

const WAKTU = new Intl.DateTimeFormat('id-ID', {
  dateStyle: 'medium',
  timeStyle: 'short',
  timeZone: ZONA,
});

export function waktu(iso: string): string {
  return WAKTU.format(new Date(iso));
}

/**
 * Waktu untuk berkas ekspor: `2026-09-16 14:05`, zona toko.
 *
 * Locale `sv-SE` dipilih bukan karena bahasanya, melainkan karena
 * formatnya -- ISO 8601 tanpa huruf, satu-satunya bentuk yang diurutkan
 * benar oleh spreadsheet apa pun dan tetap terbaca manusia.
 */
const WAKTU_EKSPOR = new Intl.DateTimeFormat('sv-SE', {
  dateStyle: 'short',
  timeStyle: 'short',
  timeZone: ZONA,
});

export function waktuEkspor(iso: string): string {
  return WAKTU_EKSPOR.format(new Date(iso));
}

const TANGGAL = new Intl.DateTimeFormat('id-ID', { dateStyle: 'medium' });

/** Tanggal tanpa jam, untuk nilai `DATE` seperti kedaluwarsa batch. */
export function tanggal(iso: string): string {
  // Dibaca sebagai tanggal lokal, bukan UTC tengah malam: `new Date('2026-01-01')`
  // di zona WIB menggeser tampilan jadi 1 Januari pukul 07.00, dan tanggal
  // kedaluwarsa yang meleset sehari bukan kesalahan yang boleh dibiarkan.
  const [tahun, bulan, hari] = iso.slice(0, 10).split('-').map(Number);
  return TANGGAL.format(new Date(tahun, bulan - 1, hari));
}

/** Sisa hari menuju sebuah tanggal. Negatif berarti sudah lewat. */
export function sisaHari(iso: string): number {
  const [tahun, bulan, hari] = iso.slice(0, 10).split('-').map(Number);
  const target = new Date(tahun, bulan - 1, hari);
  const kini = new Date();
  const hariIni = new Date(kini.getFullYear(), kini.getMonth(), kini.getDate());
  return Math.round((target.getTime() - hariIni.getTime()) / 86_400_000);
}
