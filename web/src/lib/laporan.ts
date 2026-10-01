/** Saringan laporan penjualan, dibaca sama persis oleh halaman laporan dan ekspor agar "Ekspor" tak mengunduh periode berbeda dari yang terlihat. */

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
  /** Periode kartu Produk Terlaris terpisah dari `periode`; string kosong berarti ikut `periode` (lihat `kueriApi`). */
  top_periode: string;
}

/** Nilai baku periode. Sama dengan baku di backend. */
const PERIODE_BAKU = 'month';

/** Nomor pesanan tampil = 8 karakter awal UUID transaksi huruf besar (bukan kolom DB); dipakai di tabel, CSV, dan detail agar ketiganya menunjuk nomor yang sama. */
export function nomorPesanan(id: string): string {
  return id.slice(0, 8).toUpperCase();
}

function sah(nilai: string | null, pilihan: readonly { nilai: string }[]): string {
  return nilai && pilihan.some((p) => p.nilai === nilai) ? nilai : '';
}

/** Nilai saringan dari URL yang tak dikenal dibuang di sini agar backend tak menerima tebakan dari URL ketikan tangan. */
export function bacaSaringan(url: URL): Saringan {
  const p = url.searchParams;
  return {
    periode: sah(p.get('periode'), PERIODE) || PERIODE_BAKU,
    metode: sah(p.get('metode'), METODE),
    jenis: sah(p.get('jenis'), JENIS),
    top_periode: sah(p.get('top_periode'), PERIODE),
  };
}

/** Saringan sebagai query untuk backend. `batas` = banyaknya baris penjualan. */
export function kueriApi(s: Saringan, batas: number): string {
  const q = new URLSearchParams({ period: s.periode, sales_limit: String(batas) });
  if (s.metode) q.set('payment_method', s.metode);
  if (s.jenis) q.set('type', s.jenis);
  // Kosong berarti "ikut periode utama", yang juga jawaban backend bila `top_period` tak dikirim.
  if (s.top_periode) q.set('top_period', s.top_periode);
  return q.toString();
}

/** Saringan sebagai query untuk tautan di dalam aplikasi. */
export function kueriHalaman(s: Saringan): string {
  const q = new URLSearchParams();
  if (s.periode !== PERIODE_BAKU) q.set('periode', s.periode);
  if (s.metode) q.set('metode', s.metode);
  if (s.jenis) q.set('jenis', s.jenis);
  if (s.top_periode) q.set('top_periode', s.top_periode);
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
