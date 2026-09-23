/**
 * Menyajikan foto produk dari Azure Blob Storage.
 *
 * Kontainernya privat, jadi `<img src>` tidak bisa menunjuk Azure langsung.
 * Rute ini yang mengambilkannya -- lihat `lib/blob.ts` untuk alasan lengkap
 * dan untuk penyaringan nama blobnya.
 *
 * Sesi tetap diperiksa middleware: rutenya terdaftar sebagai halaman umum,
 * jadi ketiga peran boleh membukanya, tapi bukan orang yang belum masuk.
 */
import type { APIRoute } from 'astro';
import { ambilFoto } from '../../lib/blob';

export const GET: APIRoute = async ({ params }) => {
  const foto = await ambilFoto(params.nama ?? '');
  return foto ?? new Response('Foto tidak ditemukan.', { status: 404 });
};
