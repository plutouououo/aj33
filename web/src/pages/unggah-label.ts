/** Penerima PDF label Shopee dari Shortcut iPhone (iOS tak mendukung Web Share Target): badan permintaan berisi PDF mentah, `Authorization: Bearer` berisi token unggah dari Pengaturan › Shortcut iPhone, jawabannya `{ pesan }` untuk notifikasi Shortcut. */
import type { APIRoute } from 'astro';
import { api, ApiRequestError, type HasilLabel } from '../lib/api';
import { bacaLabelPdf } from '../lib/label-shopee';

const BATAS_BYTE = 10 * 1024 * 1024;

const jawab = (status: number, pesan: string) => Response.json({ pesan }, { status });

// Galat tak terduga tetap dijawab JSON, karena halaman galat HTML membuat Shortcut gagal membaca `pesan` tanpa keterangan apa pun.
export const POST: APIRoute = async (context) => {
  try {
    return await terima(context.request);
  } catch (err) {
    console.error('unggah-label gagal', err);
    return jawab(500, 'Server sedang bermasalah. Coba lagi sebentar lagi.');
  }
};

/** Badan dibaca sampai `BATAS_BYTE`; `null` bila lebih, tanpa menunggu sisanya. */
async function bacaBadan(request: Request): Promise<Uint8Array | null> {
  if (Number(request.headers.get('content-length') ?? 0) > BATAS_BYTE) return null;
  if (!request.body) return new Uint8Array();
  const potongan: Uint8Array[] = [];
  let panjang = 0;
  for await (const p of request.body) {
    panjang += p.length;
    if (panjang > BATAS_BYTE) return null;
    potongan.push(p);
  }
  const data = new Uint8Array(panjang);
  let posisi = 0;
  for (const p of potongan) {
    data.set(p, posisi);
    posisi += p.length;
  }
  return data;
}

async function terima(request: Request): Promise<Response> {
  // Badan dihabiskan sebelum menjawab galat apa pun, karena iOS menampilkan jawaban yang datang di tengah unggahan sebagai "The network connection was lost".
  const data = await bacaBadan(request);
  if (!data) return jawab(413, `Ukuran PDF maksimal ${BATAS_BYTE / (1024 * 1024)} MB.`);

  const token = request.headers.get('authorization')?.match(/^Bearer\s+(\S+)$/)?.[1];
  if (!token) return jawab(401, 'Token unggah belum dipasang di Shortcut.');

  // Token diperiksa sebelum PDF dibaca, karena membaca PDF berat dan tak boleh bisa dipicu siapa pun.
  try {
    await api('/orders/labels/shortcut', { token });
  } catch (err) {
    if (err instanceof ApiRequestError && err.status === 401) return jawab(401, err.message);
    throw err;
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
}
