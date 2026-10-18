/** Web Push ke perangkat pengepak; kunci VAPID dibaca dari `process.env` saat request (lihat `api.ts`), dan tanpa kunci notifikasi mati diam-diam karena tiket tetap terkirim. */
import webpush from 'web-push';
import { api, type LanggananPush } from './api';

/** `import.meta.env` hanya cadangan untuk `astro dev` (Vite tak memuat `.env` ke `process.env`); ditulis per nama karena Vite menggantinya secara statis. */
const CADANGAN: Record<string, string | undefined> = {
  VAPID_PUBLIC_KEY: import.meta.env.VAPID_PUBLIC_KEY,
  VAPID_PRIVATE_KEY: import.meta.env.VAPID_PRIVATE_KEY,
  VAPID_SUBJECT: import.meta.env.VAPID_SUBJECT,
};

function env(nama: string): string | undefined {
  return (typeof process !== 'undefined' ? process.env[nama] : undefined) || CADANGAN[nama] || undefined;
}

/** Kunci publik untuk `pushManager.subscribe` di browser; `null` bila server belum disetel. */
export function kunciPublikVapid(): string | null {
  return env('VAPID_PUBLIC_KEY') && env('VAPID_PRIVATE_KEY') && env('VAPID_SUBJECT')
    ? env('VAPID_PUBLIC_KEY')!
    : null;
}

export interface IsiNotifikasi {
  title: string;
  body: string;
  url: string;
}

/** Kegagalan dicatat, tidak dilempar: notifikasi pelengkap, tiket sudah tersimpan saat fungsi ini dipanggil. */
export async function kirimNotifikasi(token: string, userId: string, isi: IsiNotifikasi): Promise<void> {
  const publik = kunciPublikVapid();
  if (!publik) return;

  try {
    const langganan = await api<LanggananPush[]>(`/users/${userId}/push-subscriptions`, { token });
    const opsi = {
      vapidDetails: { subject: env('VAPID_SUBJECT')!, publicKey: publik, privateKey: env('VAPID_PRIVATE_KEY')! },
      TTL: 60 * 60,
    };

    await Promise.all(
      langganan.map(async (l) => {
        try {
          await webpush.sendNotification(
            { endpoint: l.endpoint, keys: { p256dh: l.p256dh, auth: l.auth } },
            JSON.stringify(isi),
            opsi,
          );
        } catch (err) {
          // 404/410 berarti perangkat sudah mencabut langganannya; baris itu dibuang agar tak dicoba terus.
          const status = (err as { statusCode?: number }).statusCode;
          if (status === 404 || status === 410) {
            await api('/push-subscriptions', { method: 'DELETE', token, body: { endpoint: l.endpoint } });
          } else {
            console.error('Notifikasi gagal terkirim', status, err);
          }
        }
      }),
    );
  } catch (err) {
    console.error('Notifikasi gagal terkirim', err);
  }
}
