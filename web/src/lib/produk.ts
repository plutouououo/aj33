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
const SARINGAN_DAFTAR = ['cari', 'kategori', 'tab', 'urut', 'hanya_menipis', 'per', 'halaman'] as const;

/**
 * Tautan kembali ke daftar produk dengan saringan dan halaman yang sama
 * seperti saat pengguna berangkat. Tanpa ini tombol "Kembali" selalu
 * mendarat di halaman satu tanpa saringan, dan pekerjaan menyunting banyak
 * produk berarti menyetel ulang saringan setiap kali.
 *
 * `pesan` adalah kode kabar yang dititipkan ke daftar -- dipakai halaman
 * tambah dan sunting yang berakhir dengan pengalihan ke sini, supaya
 * kabar berhasilnya muncul di tempat pengguna mendarat, bukan di halaman
 * yang barusan ia tinggalkan.
 */
export function kembaliKeDaftar(dari: string | null | undefined, pesan?: string): string {
  const asal = new URLSearchParams(dari ?? '');
  const p = new URLSearchParams();
  for (const kunci of SARINGAN_DAFTAR) {
    const nilai = asal.get(kunci);
    if (nilai) p.set(kunci, nilai);
  }
  if (pesan) p.set('pesan', pesan);
  const q = p.toString();
  return q ? `/produk?${q}` : '/produk';
}
