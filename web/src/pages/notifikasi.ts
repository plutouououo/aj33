/** Penerus langganan Web Push dari skrip halaman tiket ke backend, karena browser tak pernah memanggil backend langsung. */
import type { APIRoute } from 'astro';
import { api, ApiRequestError } from '../lib/api';

export const POST: APIRoute = async ({ request, locals }) => {
  try {
    await api('/push-subscriptions', { method: 'POST', token: locals.token, body: await request.json() });
    return new Response(null, { status: 204 });
  } catch (err) {
    const status = err instanceof ApiRequestError ? err.status : 500;
    return new Response(null, { status });
  }
};

export const DELETE: APIRoute = async ({ request, locals }) => {
  try {
    await api('/push-subscriptions', { method: 'DELETE', token: locals.token, body: await request.json() });
  } catch {
    // Langganan yang gagal dihapus akan dibuang sendiri saat layanan push menjawab 410.
  }
  return new Response(null, { status: 204 });
};
