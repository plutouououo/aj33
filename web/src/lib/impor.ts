/** Batas unggahan impor harus sama persis dengan `backend/src/import/parse.rs` (tak bisa berbagi konstanta lintas bahasa); server tetap gerbang sungguhan. */
export const BATAS_BYTE = 10 * 1024 * 1024;
export const EKSTENSI_DIIZINKAN = ['.xlsx', '.csv'];

import type { ImportStatus } from './api';

/** Kelas lencana dari theme.css dipetakan ke status batch; `committed` dengan `fail_count > 0` dapat lencana bahaya karena ada baris yang butuh `retry`. */
export function kelasLencanaImpor(status: ImportStatus, failCount: number): string {
  switch (status) {
    case 'draft':
    case 'pending_review':
    case 'approved':
      return 'lencana-perlu-tindakan';
    case 'committing':
      return 'lencana-berjalan';
    case 'committed':
      return failCount > 0 ? 'lencana-gagal' : 'lencana-selesai';
    case 'cancelled':
      return 'lencana-nonaktif';
  }
}

export function labelStatusImpor(status: ImportStatus): string {
  switch (status) {
    case 'draft':
      return 'Draf';
    case 'pending_review':
      return 'Menunggu Tinjauan';
    case 'approved':
      return 'Disetujui';
    case 'committing':
      return 'Sedang Diproses';
    case 'committed':
      return 'Selesai';
    case 'cancelled':
      return 'Dibatalkan';
  }
}
