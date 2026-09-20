/**
 * Sesi login disimpan di cookie httpOnly.
 *
 * Proyek lama menyimpan JWT di sisi client. SSR membutuhkan token yang
 * terbaca server saat merender, dan cookie httpOnly sekaligus menutup jalan
 * bagi skrip apa pun di halaman untuk membacanya -- token tidak bisa dicuri
 * lewat XSS.
 */
import type { AstroCookies } from 'astro';
import { COOKIE_SESI, type Role, type User } from './api';

/** Sama dengan masa berlaku JWT di backend (8 jam). */
const UMUR_DETIK = 8 * 60 * 60;

export function simpanSesi(cookies: AstroCookies, token: string): void {
  cookies.set(COOKIE_SESI, token, {
    httpOnly: true,
    // `lax` membuat cookie tetap terkirim saat pengguna membuka tautan ke
    // aplikasi dari luar, tapi tidak ikut pada request lintas situs yang
    // mengubah data -- penjagaan CSRF untuk form POST di aplikasi ini.
    sameSite: 'lax',
    path: '/',
    maxAge: UMUR_DETIK,
    secure: import.meta.env.PROD,
  });
}

export function hapusSesi(cookies: AstroCookies): void {
  cookies.delete(COOKIE_SESI, { path: '/' });
}

export function ambilToken(cookies: AstroCookies): string | undefined {
  return cookies.get(COOKIE_SESI)?.value;
}

/** Halaman yang boleh dibuka tanpa login. */
export const HALAMAN_PUBLIK = ['/login', '/offline'];

export function bolehTanpaLogin(pathname: string): boolean {
  return HALAMAN_PUBLIK.includes(pathname);
}

/** Halaman pertama tiap peran setelah login. */
export function berandaUntuk(role: Role): string {
  switch (role) {
    case 'kasir':
      return '/kasir';
    case 'pengepak':
      return '/tiket';
    case 'owner':
      return '/dasbor';
  }
}

/**
 * Halaman yang wajib bisa dibuka pengguna mana pun yang sudah login.
 *
 * Halaman publik ikut di sini supaya pengguna yang sudah login tidak
 * dipantulkan dari `/login` oleh penjaga peran -- halaman itu sendiri yang
 * mengarahkannya ke beranda, dan pesan "kamu sudah masuk" lebih berguna
 * daripada lompatan diam-diam.
 */
const HALAMAN_UMUM = ['/', '/logout', '/ganti-password', ...HALAMAN_PUBLIK];

/**
 * Halaman yang boleh dibuka tiap peran, sebagai awalan path.
 *
 * INI SATU-SATUNYA DAFTARNYA. Middleware memakainya untuk memantulkan
 * request, dan AppShell memakainya untuk menyusun menu -- kalau keduanya
 * punya daftar sendiri, cepat atau lambat menu akan menawarkan halaman yang
 * lalu ditolak penjaganya, atau lebih buruk: menyembunyikan halaman yang
 * sebenarnya masih terbuka bagi siapa pun yang mengetik alamatnya.
 *
 * Pengepak boleh membuka /kasir HANYA untuk membaca detail produk; halaman
 * itu sendiri yang menyembunyikan kendali transaksinya, dan backend menolak
 * checkout dari pengepak apa pun yang dikirim frontend.
 */
const AKSES: Record<Role, readonly string[] | 'semua'> = {
  owner: 'semua',
  kasir: ['/kasir'],
  pengepak: ['/tiket', '/kasir'],
};

function cocok(daftar: readonly string[], pathname: string): boolean {
  return daftar.some((awalan) => pathname === awalan || pathname.startsWith(`${awalan}/`));
}

export function bolehAkses(role: Role, pathname: string): boolean {
  if (cocok(HALAMAN_UMUM, pathname)) return true;
  const izin = AKSES[role];
  return izin === 'semua' || cocok(izin, pathname);
}

/**
 * Akun berpassword sementara ditahan di halaman ganti password.
 *
 * Logout tetap dibiarkan lewat: orang yang salah masuk ke akun bukan miliknya
 * harus tetap bisa keluar tanpa mengganti password siapa pun.
 */
export function harusGantiPassword(user: User, pathname: string): boolean {
  return user.must_change_password && pathname !== '/ganti-password' && pathname !== '/logout';
}
