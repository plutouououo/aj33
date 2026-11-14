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

/** Nilai `periode` bila rentang tanggal dipilih sendiri lewat `dari`/`sampai`. */
export const PERIODE_TANGGAL = 'custom';

export interface Saringan {
  /** Salah satu `PERIODE`, atau `PERIODE_TANGGAL` bila `dari`/`sampai` terisi. */
  periode: string;
  /** `YYYY-MM-DD` atau kosong; hanya terisi bila `periode` adalah `PERIODE_TANGGAL`, dan kosong berarti sisi itu tak dibatasi. */
  dari: string;
  sampai: string;
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

/** Tanggal `YYYY-MM-DD` yang benar-benar ada di kalender (bukan sekadar berbentuk), selain itu kosong. */
function tanggalSah(nilai: string | null): string {
  if (!nilai || !/^\d{4}-\d{2}-\d{2}$/.test(nilai)) return '';
  const d = new Date(`${nilai}T00:00:00Z`);
  return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === nilai ? nilai : '';
}

/** Nilai saringan dari URL yang tak dikenal dibuang di sini agar backend tak menerima tebakan dari URL ketikan tangan. */
export function bacaSaringan(url: URL): Saringan {
  const p = url.searchParams;
  let dari = tanggalSah(p.get('dari'));
  let sampai = tanggalSah(p.get('sampai'));
  // Tanggal terbalik ditukar, bukan ditolak: pengguna jelas bermaksud rentang itu, dan backend menolak dari > sampai.
  if (dari && sampai && dari > sampai) [dari, sampai] = [sampai, dari];
  // Tanggal yang terisi menang atas pilihan periode, supaya form tanpa JavaScript tak diam-diam mengabaikannya.
  const tanggal = Boolean(dari || sampai);
  return {
    periode: tanggal ? PERIODE_TANGGAL : sah(p.get('periode'), PERIODE) || PERIODE_BAKU,
    dari,
    sampai,
    metode: sah(p.get('metode'), METODE),
    jenis: sah(p.get('jenis'), JENIS),
    top_periode: sah(p.get('top_periode'), PERIODE),
  };
}

/** Jendela satu daftar: `batas` baris mulai dari `lewati`. */
export interface Jendela {
  batas: number;
  lewati: number;
}

/** Hanya rentang waktu saringan sebagai query backend (`period` atau `from`/`to`), untuk laporan yang tak mengenal metode bayar dan jenis pembeli seperti laba rugi dan beban. */
export function kueriPeriodeApi(s: Saringan): URLSearchParams {
  const q = new URLSearchParams();
  if (s.periode === PERIODE_TANGGAL) {
    if (s.dari) q.set('from', s.dari);
    if (s.sampai) q.set('to', s.sampai);
  } else {
    q.set('period', s.periode);
  }
  return q;
}

/** Saringan sebagai query untuk backend; tanpa `terlaris` backend memakai ukuran bakunya. */
export function kueriApi(s: Saringan, penjualan: Jendela, terlaris?: Jendela): string {
  const q = kueriPeriodeApi(s);
  q.set('sales_limit', String(penjualan.batas));
  q.set('sales_offset', String(penjualan.lewati));
  if (terlaris) {
    q.set('top_limit', String(terlaris.batas));
    q.set('top_offset', String(terlaris.lewati));
  }
  if (s.metode) q.set('payment_method', s.metode);
  if (s.jenis) q.set('type', s.jenis);
  // Kosong berarti "ikut periode utama", yang juga jawaban backend bila `top_period` tak dikirim.
  if (s.top_periode) q.set('top_period', s.top_periode);
  return q.toString();
}

/** Hanya rentang waktu saringan sebagai query tautan, dibawa tab laporan agar pindah tab tak mengganti periode. */
export function kueriPeriodeHalaman(s: Saringan): URLSearchParams {
  const q = new URLSearchParams();
  if (s.periode === PERIODE_TANGGAL) {
    if (s.dari) q.set('dari', s.dari);
    if (s.sampai) q.set('sampai', s.sampai);
  } else if (s.periode !== PERIODE_BAKU) {
    q.set('periode', s.periode);
  }
  return q;
}

/** Saringan sebagai query untuk tautan di dalam aplikasi. */
export function kueriHalaman(s: Saringan): string {
  const q = kueriPeriodeHalaman(s);
  if (s.metode) q.set('metode', s.metode);
  if (s.jenis) q.set('jenis', s.jenis);
  if (s.top_periode) q.set('top_periode', s.top_periode);
  return q.toString();
}

const BULAN = ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'];

/** `2026-11-05` menjadi `5 Nov 2026`. */
function labelTanggal(nilai: string): string {
  const [tahun, bulan, hari] = nilai.split('-').map(Number);
  return `${hari} ${BULAN[bulan - 1]} ${tahun}`;
}

/** Nama rentang untuk judul kartu dan CSV: label periode, atau tanggal pilihan. */
export function labelPeriode(s: Saringan): string {
  if (s.periode !== PERIODE_TANGGAL) return PERIODE.find((p) => p.nilai === s.periode)?.label ?? s.periode;
  if (s.dari && s.sampai) return s.dari === s.sampai ? labelTanggal(s.dari) : `${labelTanggal(s.dari)} – ${labelTanggal(s.sampai)}`;
  return s.dari ? `Sejak ${labelTanggal(s.dari)}` : `Sampai ${labelTanggal(s.sampai)}`;
}

/** Potongan nama berkas CSV untuk rentang ini. */
export function kodePeriode(s: Saringan): string {
  return s.periode === PERIODE_TANGGAL ? `${s.dari || 'awal'}_${s.sampai || 'kini'}` : s.periode;
}

export function labelMetode(nilai: string): string {
  return METODE.find((m) => m.nilai === nilai)?.label ?? nilai;
}

export function labelKanal(nilai: string): string {
  return { toko: 'Offline (Toko)', shopee: 'Shopee', tiktok: 'Tokopedia' }[nilai] ?? nilai;
}

/** Pilihan alasan pengurangan stok, urutan tampil form koreksi; harus sama dengan `AlasanKoreksi` di `backend/src/stock.rs`. */
export const ALASAN_KOREKSI = [
  { nilai: 'rusak', label: 'Rusak', singkat: 'Rusak' },
  { nilai: 'kedaluwarsa', label: 'Kedaluwarsa', singkat: 'Kedaluwarsa' },
  { nilai: 'hilang', label: 'Hilang (selisih opname)', singkat: 'Hilang' },
  { nilai: 'salah_catat', label: 'Salah catat (bukan kerugian)', singkat: 'Salah catat' },
] as const;

export function labelAlasan(nilai: string | null): string {
  if (nilai === null) return 'Tanpa alasan';
  return ALASAN_KOREKSI.find((a) => a.nilai === nilai)?.label ?? nilai;
}

export function labelJenis(nilai: string): string {
  return JENIS.find((j) => j.nilai === nilai)?.label ?? nilai;
}
