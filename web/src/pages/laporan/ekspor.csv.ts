/**
 * Unduhan laporan penjualan sebagai CSV.
 *
 * Dibuat di server, bukan di browser: seluruh aplikasi ini bekerja tanpa
 * JavaScript, dan ekspor yang hanya jalan kalau JS hidup akan jadi satu-
 * satunya fitur yang diam-diam hilang. Sebagai tautan biasa, ekspor juga
 * ikut bekerja di tab baru, "simpan tautan sebagai", dan riwayat unduhan.
 *
 * SARINGANNYA DIBACA DARI URL YANG SAMA dengan halaman laporan (lihat
 * `lib/laporan.ts`), jadi berkas yang terunduh tidak pernah berisi periode
 * yang berbeda dari yang sedang dilihat.
 *
 * CSV, bukan XLSX. Keduanya sama-sama terbuka di Excel dan Google Sheets;
 * CSV tidak menuntut pustaka penulis berkas biner ikut terpasang di server.
 */
import type { APIRoute } from 'astro';
import { api, ApiRequestError, type SalesReport } from '../../lib/api';
import {
  bacaSaringan,
  kueriApi,
  labelJenis,
  labelMetode,
  labelPeriode,
} from '../../lib/laporan';
import { waktuEkspor } from '../../lib/format';
import { ambilToken } from '../../lib/session';

/**
 * Batas baris yang ikut terunduh. Sama dengan batas atas backend: lebih dari
 * ini akan dipotong diam-diam, dan berkas yang tidak lengkap tanpa ada yang
 * memberitahu lebih buruk daripada berkas yang besar.
 */
const BARIS_MAKS = 1000;

/**
 * Menyiapkan satu sel CSV.
 *
 * Tanda kutip, koma, dan baris baru harus dikutip -- nama pelanggan dan
 * kategori beban diketik bebas, jadi ketiganya benar-benar bisa muncul.
 */
function sel(nilai: string | number | null): string {
  const teks = nilai === null ? '' : String(nilai);
  return /[",\r\n]/.test(teks) ? `"${teks.replaceAll('"', '""')}"` : teks;
}

function baris(nilai: (string | number | null)[]): string {
  return nilai.map(sel).join(',');
}

export const GET: APIRoute = async ({ url, cookies, redirect }) => {
  // Middleware melewatkan alamat yang mengandung titik (dianggap aset), jadi
  // `locals.user` tidak terisi di sini dan sesi diperiksa sendiri.
  const token = ambilToken(cookies);
  if (!token) return redirect('/login', 302);

  const saringan = bacaSaringan(url);

  let laporan: SalesReport;
  try {
    laporan = await api<SalesReport>(`/reports/sales?${kueriApi(saringan, BARIS_MAKS)}`, { token });
  } catch (err) {
    // 401 sesi habis, 403 bukan owner. Keduanya dijawab dengan mengembalikan
    // pengguna ke aplikasi, bukan dengan berkas berisi pesan galat.
    if (err instanceof ApiRequestError && (err.status === 401 || err.status === 403)) {
      return redirect(err.status === 401 ? '/login' : '/', 302);
    }
    throw err;
  }

  const { summary, sales } = laporan;

  const isi = [
    baris(['Laporan Penjualan Toko Ayam Aneka Jaya 33']),
    baris(['Periode', labelPeriode(saringan.periode)]),
    baris(['Metode bayar', saringan.metode ? labelMetode(saringan.metode) : 'Semua']),
    baris(['Jenis pembeli', saringan.jenis ? labelJenis(saringan.jenis) : 'Semua']),
    baris(['Diunduh', waktuEkspor(new Date().toISOString())]),
    '',
    baris(['Ringkasan', 'Nilai']),
    baris(['Omzet barang', summary.revenue]),
    baris(['Ongkir ditagihkan', summary.shipping]),
    baris(['Harga pokok', summary.cogs]),
    baris(['Beban', summary.expenses]),
    baris(['Laba', summary.profit]),
    baris(['Jumlah transaksi', summary.transaction_count]),
    baris(['Item tanpa harga pokok', summary.items_without_cost]),
    '',
    baris([
      'Waktu',
      'ID Transaksi',
      'Pembeli',
      'Jenis',
      'Metode Bayar',
      'Jumlah Item',
      'Omzet Barang',
      'Ongkir',
      'Dibayar',
    ]),
    ...sales.map((s) =>
      baris([
        waktuEkspor(s.created_at),
        s.id,
        s.customer_name,
        labelJenis(s.type),
        labelMetode(s.payment_method),
        s.item_count,
        s.revenue,
        s.shipping,
        s.total_amount,
      ]),
    ),
  ].join('\r\n');

  const namaBerkas = `laporan-penjualan-${saringan.periode}-${waktuEkspor(new Date().toISOString()).slice(0, 10)}.csv`;

  return new Response(
    // BOM di depan. Tanpa itu Excel di Windows membaca berkas sebagai
    // encoding lokal dan nama pelanggan berhuruf non-ASCII jadi rusak.
    `﻿${isi}`,
    {
      headers: {
        'Content-Type': 'text/csv; charset=utf-8',
        'Content-Disposition': `attachment; filename="${namaBerkas}"`,
        // Laporan berubah tiap ada transaksi baru; tidak ada yang boleh
        // menyimpannya sebagai jawaban untuk permintaan berikutnya.
        'Cache-Control': 'no-store',
      },
    },
  );
};
