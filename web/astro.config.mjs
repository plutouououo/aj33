// @ts-check
import { defineConfig } from 'astro/config';
import node from '@astrojs/node';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  // Setiap halaman dirender di server saat diminta. Inilah sumber kecepatan
  // yang jadi alasan refactor ini: browser menerima HTML jadi, bukan bundle
  // JavaScript yang harus dijalankan dulu sebelum ada isi yang terlihat.
  output: 'server',
  adapter: node({ mode: 'standalone' }),
  server: { port: 4321 },
  vite: {
    plugins: [tailwindcss()],
  },
});
