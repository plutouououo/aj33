// @ts-check
import { defineConfig } from 'astro/config';
import node from '@astrojs/node';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  // Setiap halaman dirender di server saat diminta. Inilah sumber kecepatan
  // yang jadi alasan refactor ini: browser menerima HTML jadi, bukan bundle
  // JavaScript yang harus dijalankan dulu sebelum ada isi yang terlihat.
  //
  // Halaman ini juga WAJIB dirender per permintaan, bukan sekadar lebih cepat:
  // sesi dibaca dari cookie httpOnly saat render, jadi tidak ada halaman yang
  // bisa dibekukan jadi HTML statis.
  output: 'server',

  // `standalone` menghasilkan server Node yang berdiri sendiri di
  // dist/server/entry.mjs -- itulah yang dijalankan service systemd di VPS.
  // Alamat dan portnya dibaca dari env HOST dan PORT saat start.
  //
  // Versi adapter harus sejalan dengan versi Astro: baris @9 untuk Astro 5.
  // `npx astro add node` bisa menarik versi untuk Astro 7 dan build langsung
  // berhenti dengan galat peer dependency.
  adapter: node({ mode: 'standalone' }),

  // Domain yang boleh dipercaya sebagai identitas situs ini.
  //
  // WAJIB diisi di balik reverse proxy. Sejak Astro 5.14, header `Host` dan
  // `X-Forwarded-Host` TIDAK dipercaya kalau daftar ini kosong -- pengetatan
  // terhadap host header injection. Akibatnya hostname jatuh ke nilai cadangan
  // `localhost`, sehingga `Astro.url` jadi `https://localhost/...`, tidak cocok
  // dengan header `Origin` dari browser, dan SEMUA form POST ditolak dengan
  // "Cross-site POST form submissions are forbidden" -- termasuk halaman login.
  //
  // localhost ikut didaftarkan supaya `npm run preview` di mesin sendiri tetap
  // bisa mengirim form.
  security: {
    allowedDomains: [
      { hostname: 'tokoayamaj33.my.id', protocol: 'https' },
      { hostname: 'www.tokoayamaj33.my.id', protocol: 'https' },
      { hostname: 'localhost', protocol: 'http' },
    ],
  },

  server: { port: 4321 },
  vite: {
    plugins: [tailwindcss()],
  },
});
