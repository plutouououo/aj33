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
  'harga grosir',
  'harga shopee',
  'grosir shopee',
  'harga tokopedia',
  'grosir tokopedia',
  'batas grosir',
  'modal',
  'margin',
  'stok',
  'terbit',
];

const CONTOH: (string | number)[][] = [
  ['Ceker Bersih', '', 'Ceker', 'AFC', 'Ceker', 'Super Besar', '2 kg', 28000, 25000, 30000, 27000, '', '', '10 kg', 19600, 30, 10, 'ya'],
  ['Dada Fillet', 'DADA-1KG', 'Dada', 'AFC', 'Fillet', 'Standar', '1 kg', 'Rp 45.000', '', '', '', 47000, 44000, '', '', '', 5, 'ya'],
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
