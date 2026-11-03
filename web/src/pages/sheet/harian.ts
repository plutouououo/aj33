/** Penerus laporan harian ke Apps Script di Google Sheets, karena hanya web yang menghadap internet; token sheet diperiksa backend, bukan di sini. */
import type { APIRoute } from 'astro';
import { api, ApiRequestError } from '../../lib/api';

export const GET: APIRoute = async ({ request, url }) => {
  const token = request.headers.get('authorization')?.match(/^Bearer\s+(\S+)$/)?.[1];
  if (!token) return Response.json({ pesan: 'Token sheet belum dipasang.' }, { status: 401 });

  const tanggal = url.searchParams.get('tanggal');
  const query = tanggal ? `?tanggal=${encodeURIComponent(tanggal)}` : '';

  try {
    return Response.json(await api(`/sheet/harian${query}`, { token }));
  } catch (err) {
    if (err instanceof ApiRequestError) {
      return Response.json({ pesan: err.message }, { status: err.status });
    }
    console.error('sheet/harian gagal', err);
    return Response.json({ pesan: 'Server sedang bermasalah.' }, { status: 500 });
  }
};
