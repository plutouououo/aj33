/**
 * Saringan laporan penjualan.
 *
 * Halaman laporan dan endpoint ekspor membaca saringan yang sama persis dari
 * URL. Ditaruh di satu berkas supaya tombol "Ekspor" tidak pernah mengunduh
 * periode yang berbeda dari yang sedang dilihat di layar.
 */

export const PERIODE = [
  { nilai: 'today', label: 'Hari Ini' },
  { nilai: 'week', label: 'Minggu Ini' },
  { nilai: 'month', label: 'Bulan Ini' },
  { nilai: 'year', label: 'Tahun Ini' },
  { nilai: 'all', label: 'Seluruh Waktu' },
] as const;

export const METODE = [
  { nilai: 'cash', label: 'Tunai' },
  { nilai: 'transfer', label: 'Transfer' },
  { nilai: 'ewallet', label: 'E-Wallet' },
] as const;

export const JENIS = [
  { nilai: 'walk_in', label: 'Pembeli Langsung' },
  { nilai: 'pre_order', label: 'Pre-order' },
] as const;

export interface Saringan {
  periode: string;
  /** String kosong berarti "semua". */
  metode: string;
  jenis: string;
}

/** Nilai baku periode. Sama dengan baku di backend. */
const PERIODE_BAKU = 'month';

function sah(nilai: string | null, pilihan: readonly { nilai: string }[]): string {
  return nilai && pilihan.some((p) => p.nilai === nilai) ? nilai : '';
}

/**
 * Membaca saringan dari URL. Nilai yang tidak dikenal dibuang di sini, jadi
 * backend tidak pernah menerima tebakan dari URL yang diketik tangan.
 */
export function bacaSaringan(url: URL): Saringan {
  const p = url.searchParams;
  return {
    periode: sah(p.get('periode'), PERIODE) || PERIODE_BAKU,
    metode: sah(p.get('metode'), METODE),
    jenis: sah(p.get('jenis'), JENIS),
  };
}

/** Saringan sebagai query untuk backend. `batas` = banyaknya baris penjualan. */
export function kueriApi(s: Saringan, batas: number): string {
  const q = new URLSearchParams({ period: s.periode, sales_limit: String(batas) });
  if (s.metode) q.set('payment_method', s.metode);
  if (s.jenis) q.set('type', s.jenis);
  return q.toString();
}

/** Saringan sebagai query untuk tautan di dalam aplikasi. */
export function kueriHalaman(s: Saringan): string {
  const q = new URLSearchParams();
  if (s.periode !== PERIODE_BAKU) q.set('periode', s.periode);
  if (s.metode) q.set('metode', s.metode);
  if (s.jenis) q.set('jenis', s.jenis);
  return q.toString();
}

export function labelPeriode(nilai: string): string {
  return PERIODE.find((p) => p.nilai === nilai)?.label ?? nilai;
}

export function labelMetode(nilai: string): string {
  return METODE.find((m) => m.nilai === nilai)?.label ?? nilai;
}

export function labelJenis(nilai: string): string {
  return JENIS.find((j) => j.nilai === nilai)?.label ?? nilai;
}
