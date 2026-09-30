/**
 * Batas unggahan impor produk massal.
 *
 * Harus sama persis dengan `backend/src/import/parse.rs` -- dua tempat
 * karena Rust dan TypeScript tidak bisa berbagi konstanta lintas bahasa,
 * sama seperti `UMUR_DETIK` di `session.ts` yang mengikuti umur JWT
 * backend. Kalau salah satu diubah, ubah yang satunya.
 *
 * Server (`backend/src/import/service.rs::upload`) tetap gerbang
 * sesungguhnya -- ini cuma supaya berkas yang jelas ditolak tidak perlu
 * terunggah penuh dulu.
 */
export const BATAS_BYTE = 10 * 1024 * 1024;
export const EKSTENSI_DIIZINKAN = ['.xlsx', '.csv'];

import type { ImportStatus } from './api';

/**
 * Kelas lencana yang sudah ada di theme.css, dipetakan ke status batch.
 * `committed` dengan `fail_count > 0` dapat lencana bahaya, bukan selesai --
 * ada baris yang butuh `retry`, bukan sekadar catatan.
 */
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
