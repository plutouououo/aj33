/**
 * Pesan sukses lintas pengalihan (POST lalu redirect 303).
 *
 * Form yang membalas langsung dengan HTML membuat tombol muat-ulang peramban
 * mengirim ulang POST-nya -- pelanggan ganda, hapus dua kali. Setelah aksi
 * berhasil, halaman mengalihkan ke dirinya sendiri dengan kode pesan di
 * `?pesan=`, dan GET itulah yang menampilkannya. Saringan dan nomor halaman
 * di URL ikut terbawa karena yang diubah hanya satu parameter.
 */

/** Alamat tujuan sesudah aksi berhasil: halaman ini juga, plus kode pesannya. */
export function sesudahAksi(url: URL, kode: string): string {
  const tujuan = new URL(url);
  tujuan.searchParams.set('pesan', kode);
  return tujuan.pathname + tujuan.search;
}

/**
 * Teks pesan untuk kode di URL. Kode yang tidak ada di `daftar` diabaikan:
 * teks yang tampil tidak pernah berasal dari URL, jadi tautan buatan orang
 * lain tidak bisa menaruh kalimat sembarang di layar pemilik toko.
 */
export function pesanDari(url: URL, daftar: Record<string, string>): string | null {
  const kode = url.searchParams.get('pesan') ?? '';
  return Object.hasOwn(daftar, kode) ? daftar[kode] : null;
}
