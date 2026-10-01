// @ts-check
import { defineConfig } from 'astro/config';
import node from '@astrojs/node';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  // Setiap halaman dirender per permintaan di server: sesi dibaca dari cookie httpOnly saat render, jadi tak ada halaman yang bisa dibekukan jadi HTML statis.
  output: 'server',

  // `standalone` menghasilkan server Node mandiri (dist/server/entry.mjs) untuk systemd; versi adapter harus sejalan dengan Astro (@9 untuk Astro 5).
  adapter: node({ mode: 'standalone' }),

  // Domain tepercaya wajib di balik reverse proxy: sejak Astro 5.14 header Host tak dipercaya bila kosong sehingga semua form POST (termasuk login) ditolak; localhost agar `npm run preview` jalan.
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
