/** Template CSV impor statis (tanpa backend) tapi dijaga sesi seperti `ekspor.csv.ts`. */
import type { APIRoute } from 'astro';
import { ambilToken } from '../../../lib/session';

function sel(nilai: string | number): string {
  const teks = String(nilai);
  return /[",\r\n]/.test(teks) ? `"${teks.replaceAll('"', '""')}"` : teks;
}

function baris(nilai: (string | number)[]): string {
  return nilai.map(sel).join(',');
}

const HEADER = [
  'nama',
  'sku',
  'kategori',
  'merek',
  'jenis',
  'grade',
  'ukuran',
  'harga',
  'modal',
  'margin',
  'stok',
  'terbit',
];

const CONTOH: (string | number)[][] = [
  ['Mug Kanvas Hitam', '', 'Aksesoris', 'ACME', 'Mug', 'Standar', 'Sedang', 25000, 15000, 30, 10, 'ya'],
  ['Totebag Kanvas', 'BAG-002', 'Aksesoris', 'ACME', 'Totebag', 'Standar', 'Besar', 'Rp 45.000', '', '', 5, 'ya'],
];

export const GET: APIRoute = async ({ cookies, redirect }) => {
  const token = ambilToken(cookies);
  if (!token) return redirect('/login', 302);

  const isi = [baris(HEADER), ...CONTOH.map(baris)].join('\r\n');

  return new Response(`﻿${isi}`, {
    headers: {
      'Content-Type': 'text/csv; charset=utf-8',
      'Content-Disposition': 'attachment; filename="template-impor-produk.csv"',
      'Cache-Control': 'no-store',
    },
  });
};
