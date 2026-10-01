/** CSV dibuat di server agar ekspor jalan tanpa JavaScript dan sebagai tautan biasa; saringan dari URL yang sama dengan laporan, CSV (bukan XLSX) agar tak butuh pustaka biner. */
import type { APIRoute } from 'astro';
import { api, ApiRequestError, type SalesReport } from '../../lib/api';
import {
  bacaSaringan,
  kueriApi,
  labelJenis,
  labelMetode,
  labelPeriode,
  nomorPesanan,
} from '../../lib/laporan';
import { waktuEkspor } from '../../lib/format';
import { ambilToken } from '../../lib/session';

/** Batas baris unduhan sama dengan batas atas backend; lebih dari itu terpotong diam-diam, dan berkas tak lengkap tanpa pemberitahuan lebih buruk daripada berkas besar. */
const BARIS_MAKS = 1000;

/** Menyiapkan satu sel CSV: tanda kutip, koma, dan baris baru harus dikutip karena nama pelanggan dan kategori beban diketik bebas. */
function sel(nilai: string | number | null): string {
  const teks = nilai === null ? '' : String(nilai);
  return /[",\r\n]/.test(teks) ? `"${teks.replaceAll('"', '""')}"` : teks;
}

function baris(nilai: (string | number | null)[]): string {
  return nilai.map(sel).join(',');
}

export const GET: APIRoute = async ({ url, cookies, redirect }) => {
  // Middleware kini ikut memeriksa sesi untuk CSV; pemeriksaan token di sini tetap sebagai lapis kedua.
  const token = ambilToken(cookies);
  if (!token) return redirect('/login', 302);

  const saringan = bacaSaringan(url);

  let laporan: SalesReport;
  try {
    laporan = await api<SalesReport>(`/reports/sales?${kueriApi(saringan, { batas: BARIS_MAKS, lewati: 0 })}`, { token });
  } catch (err) {
    // 401 sesi habis, 403 bukan owner; keduanya dijawab dengan mengembalikan pengguna ke aplikasi, bukan berkas berisi pesan galat.
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
    baris(['Barang terjual (kg)', summary.sold_kg]),
    baris(['Ongkir ditagihkan', summary.shipping]),
    baris(['Ongkir ditanggung toko (termasuk di beban)', summary.shipping_subsidy]),
    baris(['Harga pokok', summary.cogs]),
    baris(['Beban', summary.expenses]),
    baris([
      'Potongan Shopee (komisi + layanan + PPh 0,5% + Rp1.250/transaksi)',
      summary.platform_fees,
    ]),
    baris(['Laba', summary.profit]),
    baris(['Jumlah transaksi', summary.transaction_count]),
    baris(['Item tanpa harga pokok', summary.items_without_cost]),
    '',
    baris([
      'No. Pesanan',
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
        nomorPesanan(s.id),
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
    // BOM di depan agar Excel di Windows tak membaca berkas sebagai encoding lokal dan merusak nama non-ASCII.
    `﻿${isi}`,
    {
      headers: {
        'Content-Type': 'text/csv; charset=utf-8',
        'Content-Disposition': `attachment; filename="${namaBerkas}"`,
        // Laporan berubah tiap ada transaksi baru, jadi tak boleh disimpan sebagai jawaban permintaan berikutnya.
        'Cache-Control': 'no-store',
      },
    },
  );
};
