/** Menyajikan foto produk dari Azure (kontainer privat, lihat `lib/blob.ts`); ketiga peran boleh membukanya tapi sesi tetap diperiksa middleware. */
import type { APIRoute } from 'astro';
import { ambilFoto, PARAM_KECIL } from '../../lib/blob';

export const GET: APIRoute = async ({ params, url }) => {
  const foto = await ambilFoto(params.nama ?? '', url.searchParams.has(PARAM_KECIL));
  return foto ?? new Response('Foto tidak ditemukan.', { status: 404 });
};
