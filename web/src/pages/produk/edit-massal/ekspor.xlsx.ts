/** Meneruskan unduhan xlsx dari backend sebagai tautan biasa; token cookie tak pernah sampai ke browser. */
import type { APIRoute } from 'astro';
import { apiBerkas, ApiRequestError } from '../../../lib/api';
import { ambilToken } from '../../../lib/session';

export const GET: APIRoute = async ({ url, cookies, redirect }) => {
  const token = ambilToken(cookies);
  if (!token) return redirect('/login', 302);

  const kelompok = url.searchParams.get('kelompok') === 'stok' ? 'stok' : 'utama';
  const kueri = new URLSearchParams({ kelompok });
  for (const kunci of ['cari', 'kategori']) {
    const nilai = url.searchParams.get(kunci)?.trim();
    if (nilai) kueri.set(kunci, nilai);
  }

  try {
    const res = await apiBerkas(`/bulk-edit/export?${kueri}`, token);
    return new Response(res.body, {
      headers: {
        'Content-Type': res.headers.get('Content-Type') ?? 'application/octet-stream',
        'Content-Disposition': res.headers.get('Content-Disposition') ?? `attachment; filename="edit-massal-${kelompok}.xlsx"`,
        'Cache-Control': 'no-store',
      },
    });
  } catch (err) {
    if (err instanceof ApiRequestError && (err.status === 401 || err.status === 403)) {
      return redirect('/produk', 302);
    }
    return new Response('Gagal membuat berkas ekspor.', { status: 502 });
  }
};
