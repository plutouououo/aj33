/** Pemformatan angka dan tanggal, satu tempat supaya konsisten di semua halaman. */

const RUPIAH = new Intl.NumberFormat('id-ID', {
  style: 'currency',
  currency: 'IDR',
  maximumFractionDigits: 0,
});

export function rupiah(nilai: number): string {
  return RUPIAH.format(nilai);
}

const WAKTU = new Intl.DateTimeFormat('id-ID', {
  dateStyle: 'medium',
  timeStyle: 'short',
});

export function waktu(iso: string): string {
  return WAKTU.format(new Date(iso));
}
