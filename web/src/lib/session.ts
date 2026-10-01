/** Sesi login di cookie httpOnly: SSR butuh token terbaca server, dan httpOnly menutup jalan skrip halaman membacanya sehingga token tak bisa dicuri lewat XSS. */
import type { AstroCookies } from 'astro';
import { COOKIE_SESI, type Role, type User } from './api';

/** Sama dengan masa berlaku JWT di backend (8 jam). */
const UMUR_DETIK = 8 * 60 * 60;

export function simpanSesi(cookies: AstroCookies, token: string): void {
  cookies.set(COOKIE_SESI, token, {
    httpOnly: true,
    // `lax` mengirim cookie saat membuka tautan dari luar tapi tidak pada request lintas situs yang mengubah data, penjagaan CSRF untuk form POST.
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

/** Halaman yang wajib terbuka bagi semua pengguna login; publik ikut agar yang sudah login tak dipantulkan dari `/login`, dan `/foto` agar kartu produk kasir tak rusak. */
const HALAMAN_UMUM = [
  '/',
  '/logout',
  '/ganti-password',
  '/pengaturan/akun',
  '/foto',
  ...HALAMAN_PUBLIK,
];

/** Satu-satunya daftar halaman per peran (awalan path) dipakai middleware dan menu AppShell agar tak menyimpang; pengepak membuka /kasir hanya untuk membaca dan backend menolak checkout-nya. */
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

/** Akun berpassword sementara ditahan di halaman ganti password; logout tetap lewat agar yang salah masuk bisa keluar tanpa mengganti password siapa pun. */
export function harusGantiPassword(user: User, pathname: string): boolean {
  return (
    user.must_change_password &&
    pathname !== '/ganti-password' &&
    pathname !== '/pengaturan/akun' &&
    pathname !== '/logout'
  );
}
