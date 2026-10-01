/** Tetapan domain katalog produk, dipakai bersama halaman tambah dan sunting. */

/** Pemasok tetap, bukan isian bebas, karena merek membentuk SKU (`afco` → `AFC`) dan salah ketik melahirkan SKU berbeda; backend tetap menerima teks bebas agar impor marketplace tak gagal. */
export const MEREK = ['AFCO', 'BEST CHICKEN', 'OK CHICK', 'ZAHRA'] as const;

/** Saringan daftar produk yang boleh dibawa pulang lewat `?dari=`; hanya kunci di sini yang diterima kembali karena titipan itu data dari URL, bukan perintah. */
const SARINGAN_DAFTAR = ['cari', 'kategori', 'tab', 'urut', 'hanya_menipis', 'per', 'halaman'] as const;

/** Tautan kembali ke daftar dengan saringan dan halaman yang sama; `pesan` adalah kode kabar agar keberhasilan tampil di tempat pengguna mendarat. */
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
