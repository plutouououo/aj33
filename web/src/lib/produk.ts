/** Tetapan domain katalog produk, dipakai bersama halaman tambah dan sunting. */

/**
 * Pemasok yang dipakai toko. Daftar tetap, bukan isian bebas: merek ikut
 * membentuk SKU (`afco` → `AFC`), jadi satu salah ketik melahirkan SKU yang
 * berbeda untuk barang yang sama.
 *
 * Backend tetap menerima teks bebas — pesanan marketplace bisa membawa merek
 * di luar daftar ini, dan menolaknya di sana akan menggagalkan impor.
 */
export const MEREK = ['AFCO', 'BEST CHICKEN', 'OK CHICK'] as const;
