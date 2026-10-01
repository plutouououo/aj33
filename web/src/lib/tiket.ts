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
  unassigned: 'Belum diambil',
  assigned: 'Sudah diambil',
  packing: 'Sedang dikemas',
  packed: 'Selesai dikemas',
  handed_over: 'Diserahkan ke kurir',
};

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

/** Satu langkah maju yang sah dari status ini, beserta bunyi tombolnya. */
export function langkahBerikutnya(
  status: TicketStatus,
): { status: TicketStatus; label: string } | null {
  switch (status) {
    case 'assigned':
      return { status: 'packing', label: 'Mulai kemas' };
    case 'packing':
      return { status: 'packed', label: 'Selesai dikemas' };
    case 'packed':
      return { status: 'handed_over', label: 'Serahkan ke kurir' };
    // `unassigned` hanya bergerak lewat penugasan, `handed_over` adalah akhir.
    default:
      return null;
  }
}

/** Tiket masih boleh berpindah tangan selama barangnya belum selesai dikemas. */
export function masihBisaDiambil(status: TicketStatus): boolean {
  return status === 'unassigned' || status === 'assigned' || status === 'packing';
}
