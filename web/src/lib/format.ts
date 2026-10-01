/** Pemformatan angka dan tanggal, satu tempat supaya konsisten di semua halaman. */

const RUPIAH = new Intl.NumberFormat('id-ID', {
  style: 'currency',
  currency: 'IDR',
  maximumFractionDigits: 0,
});

export function rupiah(nilai: number): string {
  return RUPIAH.format(nilai);
}

const KG = new Intl.NumberFormat('id-ID', { maximumFractionDigits: 3 });

/** Berat dalam kg tanpa nol di belakang koma, mis. `2 kg` atau `0,9 kg`. */
export function kg(nilai: number): string {
  return `${KG.format(nilai)} kg`;
}

/** Zona waktu toko, sama dengan backend saat memotong laporan per hari (`reports/mod.rs`); disebut eksplisit karena server produksi UTC dan jam akan meleset tujuh jam. */
const ZONA = 'Asia/Jakarta';

const WAKTU = new Intl.DateTimeFormat('id-ID', {
  dateStyle: 'medium',
  timeStyle: 'short',
  timeZone: ZONA,
});

export function waktu(iso: string): string {
  return WAKTU.format(new Date(iso));
}

/** Waktu untuk ekspor (`2026-09-16 14:05`, zona toko); locale `sv-SE` dipilih karena formatnya, ISO 8601 yang terurut benar di spreadsheet dan terbaca manusia. */
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
  // Dibaca sebagai tanggal lokal, bukan UTC tengah malam, karena `new Date('2026-01-01')` di WIB bergeser dan tanggal kedaluwarsa tak boleh meleset sehari.
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

/** Perubahan satu angka dibanding pembanding, dipakai kartu Dasbor dan Laporan agar tren dihitung dan ditampilkan sama persis. */
export interface Tren {
  arah: 'naik' | 'turun';
  /** `null` kalau angka pembanding nol -- persentase dari nol tidak berarti apa-apa. */
  persen: number | null;
}

/** `null` bila tak ada pembanding (mis. "Seluruh Waktu") atau keduanya persis sama. */
export function hitungTren(sekarang: number, dulu: number | null | undefined): Tren | null {
  if (dulu === null || dulu === undefined || sekarang === dulu) return null;
  if (dulu === 0) return { arah: sekarang > 0 ? 'naik' : 'turun', persen: null };
  const persen = ((sekarang - dulu) / Math.abs(dulu)) * 100;
  return { arah: persen >= 0 ? 'naik' : 'turun', persen: Math.abs(persen) };
}

/** Teks pil naik/turun, mis. `↑ 12.4%` atau `↓ Baru`. */
export function labelTren(t: Tren | null): string {
  if (!t) return '';
  const panah = t.arah === 'naik' ? '↑' : '↓';
  const nilai = t.persen === null ? 'Baru' : `${t.persen.toFixed(1)}%`;
  return `${panah} ${nilai}`;
}
