/** Penerima PDF label Shopee dari Shortcut iPhone (iOS tak mendukung Web Share Target): badan permintaan berisi PDF mentah, `Authorization: Bearer` berisi token unggah dari Pengaturan › Shortcut iPhone, jawabannya `{ pesan }` untuk notifikasi Shortcut. */
import type { APIRoute } from 'astro';
import { api, ApiRequestError, type HasilLabel } from '../lib/api';
import { bacaLabelPdf } from '../lib/label-shopee';

const BATAS_BYTE = 10 * 1024 * 1024;

const jawab = (status: number, pesan: string) => Response.json({ pesan }, { status });

export const POST: APIRoute = async ({ request }) => {
  const token = request.headers.get('authorization')?.match(/^Bearer\s+(\S+)$/)?.[1];
  if (!token) return jawab(401, 'Token unggah belum dipasang di Shortcut.');

  // Token diperiksa sebelum PDF dibaca, karena membaca PDF berat dan tak boleh bisa dipicu siapa pun.
  try {
    await api('/orders/labels/shortcut', { token });
  } catch (err) {
    if (err instanceof ApiRequestError && err.status === 401) return jawab(401, err.message);
    throw err;
  }

  if (Number(request.headers.get('content-length') ?? 0) > BATAS_BYTE) {
    return jawab(413, `Ukuran PDF maksimal ${BATAS_BYTE / (1024 * 1024)} MB.`);
  }
  const data = new Uint8Array(await request.arrayBuffer());
  if (data.length > BATAS_BYTE) {
    return jawab(413, `Ukuran PDF maksimal ${BATAS_BYTE / (1024 * 1024)} MB.`);
  }
  // Isian Shortcut yang keliru (Form, bukan File) mengirim multipart; tanda tangan `%PDF` memberi pesan yang menunjuk ke sana.
  if (new TextDecoder().decode(data.subarray(0, 5)) !== '%PDF-') {
    return jawab(400, 'Yang terkirim bukan PDF. Pastikan Request Body di Shortcut bertipe File.');
  }

  let baca;
  try {
    baca = await bacaLabelPdf(data);
  } catch {
    return jawab(400, 'PDF tidak bisa dibaca. Pastikan ini label dari Seller Centre Shopee.');
  }
  if (baca.labels.length === 0) {
    return jawab(
      400,
      baca.gagal.length > 0
        ? `Tidak ada label terbaca. ${baca.gagal.map((g) => `Halaman ${g.halaman}: ${g.pesan}`).join(' ')}`
        : 'PDF ini tidak memuat label pengiriman.',
    );
  }

  let hasil: HasilLabel[];
  try {
    hasil = await api<HasilLabel[]>('/orders/labels/shortcut', {
      method: 'POST',
      token,
      body: { labels: baca.labels },
    });
  } catch (err) {
    if (err instanceof ApiRequestError) return jawab(err.status, err.message);
    throw err;
  }

  const dibuat = hasil.filter((h) => h.status === 'dibuat').length;
  const sudahAda = hasil.filter((h) => h.status === 'sudah_ada').length;
  const gagal = [
    ...baca.gagal.map((g) => `Halaman ${g.halaman}: ${g.pesan}`),
    ...hasil.filter((h) => h.status === 'gagal').map((h) => `${h.order_sn}: ${h.message}`),
  ];

  const pesan = [
    `${dibuat} pesanan baru tercatat${sudahAda > 0 ? `, ${sudahAda} sudah pernah dicatat` : ''}.`,
    gagal.length > 0 ? `Gagal: ${gagal.join(' ')}` : '',
    dibuat > 0 ? 'Kirim tiketnya dari Pesanan Masuk.' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return jawab(200, pesan);
};
