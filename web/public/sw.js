/*
  Service worker.

  Cakupannya sengaja sempit: membuat aplikasi bisa dipasang, memuat aset
  statis dari cache supaya kunjungan berikutnya terasa seketika, dan
  menampilkan halaman "tidak ada koneksi" yang rapi saat jaringan mati.

  Halaman TIDAK di-cache. Semuanya dirender di server dan menampilkan stok
  serta pesanan yang berubah terus -- menyajikan salinan lama justru
  berbahaya: kasir bisa menjual barang yang stoknya sudah habis. Transaksi
  juga tidak diantre offline; itu keputusan arsitektur yang tercatat di
  rencana refactor.
*/

const VERSI = 'aj33-v1';
const HALAMAN_OFFLINE = '/offline';

const ASET_AWAL = [HALAMAN_OFFLINE, '/manifest.webmanifest', '/ikon.svg'];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(VERSI)
      .then((cache) => cache.addAll(ASET_AWAL))
      // Satu aset gagal di-cache tidak boleh menggagalkan instalasi.
      .catch(() => undefined)
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((kunci) => Promise.all(kunci.filter((k) => k !== VERSI).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  const { request } = event;

  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // Navigasi: selalu ke jaringan. Kalau gagal, tampilkan halaman offline.
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request).catch(() =>
        caches.match(HALAMAN_OFFLINE).then((r) => r ?? Response.error()),
      ),
    );
    return;
  }

  // Aset statis Astro sudah bernama unik per build, jadi aman disajikan
  // dari cache lebih dulu dan diisi saat pertama diminta.
  const asetStatis = url.pathname.startsWith('/_astro/') || ASET_AWAL.includes(url.pathname);
  if (!asetStatis) return;

  event.respondWith(
    caches.match(request).then(
      (tersimpan) =>
        tersimpan ??
        fetch(request).then((res) => {
          if (res.ok) {
            const salinan = res.clone();
            caches.open(VERSI).then((cache) => cache.put(request, salinan));
          }
          return res;
        }),
    ),
  );
});
