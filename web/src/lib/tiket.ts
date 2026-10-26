/** Tampilan status tiket mencerminkan `TicketStatus` backend; di sini hanya menentukan tombol yang ditawarkan, backend yang memutuskan karena status bisa berubah di layar pengepak lain. */
import type { TicketStatus } from './api';

export const URUTAN_STATUS: TicketStatus[] = [
  'unassigned',
  'assigned',
  'packing',
  'packed',
  'handed_over',
];

export const LABEL_STATUS: Record<TicketStatus, string> = {
  unassigned: 'Belum ditugaskan',
  assigned: 'Belum dimulai',
  packing: 'Sedang dipacking',
  packed: 'Menunggu ACC owner',
  handed_over: 'Selesai',
};

export const LABEL_PENGIRIMAN: Record<string, string> = {
  instant: 'Instan',
  same_day: 'Same Day',
  reguler: 'Reguler',
};

/** Kode yang dicari di tumpukan paket; pesanan tanpa kode pengambilan (label lama, TikTok) memakai nomor pesanannya. */
export function kodeTampil(pickupCode: string | null, nomorPesanan: string): string {
  return pickupCode ?? nomorPesanan;
}

/** Kuning menunggu tindakan, merah merek berjalan, hijau selesai, sama dengan lencana di halaman lain. */
export function kelasLencana(status: TicketStatus): string {
  switch (status) {
    case 'packing':
      return 'lencana-berjalan';
    case 'handed_over':
      return 'lencana-selesai';
    default:
      return 'lencana-perlu-tindakan';
  }
}

/** Satu langkah maju yang sah dari status ini untuk penggunanya: pemegang tugas yang mengepak, hanya owner yang menyelesaikan setelah ACC. */
export function langkahBerikutnya(
  status: TicketStatus,
  peran: { owner: boolean; pemegang: boolean },
): { status: TicketStatus; label: string } | null {
  if (status === 'assigned' && peran.pemegang) return { status: 'packing', label: 'Mulai packing' };
  if (status === 'packing' && peran.pemegang) return { status: 'packed', label: 'Barang sudah disiapkan' };
  if (status === 'packed' && peran.owner) return { status: 'handed_over', label: 'Selesaikan' };
  return null;
}

/** Tiket masih boleh berpindah tangan selama barangnya belum selesai dikemas. */
export function masihBisaDitugaskan(status: TicketStatus): boolean {
  return status === 'unassigned' || status === 'assigned' || status === 'packing';
}
