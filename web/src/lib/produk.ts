/** Tetapan domain katalog produk, dipakai bersama halaman tambah dan sunting. */

/**
 * Pemasok yang dipakai toko. Daftar tetap, bukan isian bebas: merek ikut
 * membentuk SKU (`afco` → `AFC`), jadi satu salah ketik melahirkan SKU yang
 * berbeda untuk barang yang sama.
 *
 * Backend tetap menerima teks bebas — pesanan marketplace bisa membawa merek
 * di luar daftar ini, dan menolaknya di sana akan menggagalkan impor.
 */
export const MEREK = ['AFCO', 'BEST CHICKEN', 'OK CHICK', 'ZAHRA'] as const;

/**
 * Saringan daftar produk yang boleh dibawa pulang. Halaman rincian menerima
 * titipan query string lewat `?dari=`, dan hanya kunci di sini yang diterima
 * kembali -- titipan itu datang dari URL, jadi ia data, bukan perintah.
 */
const SARINGAN_DAFTAR = ['cari', 'kategori', 'status', 'tampil', 'per', 'halaman'] as const;

/**
 * Tautan kembali ke daftar produk dengan saringan dan halaman yang sama
 * seperti saat pengguna berangkat. Tanpa ini tombol "Kembali" selalu
 * mendarat di halaman satu tanpa saringan, dan pekerjaan menyunting banyak
 * produk berarti menyetel ulang saringan setiap kali.
 */
export function kembaliKeDaftar(dari: string | null | undefined): string {
  const asal = new URLSearchParams(dari ?? '');
  const p = new URLSearchParams();
  for (const kunci of SARINGAN_DAFTAR) {
    const nilai = asal.get(kunci);
    if (nilai) p.set(kunci, nilai);
  }
  const q = p.toString();
  return q ? `/produk?${q}` : '/produk';
}
