/**
 * Sesi login disimpan di cookie httpOnly.
 *
 * Proyek lama menyimpan JWT di sisi client. SSR membutuhkan token yang
 * terbaca server saat merender, dan cookie httpOnly sekaligus menutup jalan
 * bagi skrip apa pun di halaman untuk membacanya -- token tidak bisa dicuri
 * lewat XSS.
 */
import type { AstroCookies } from 'astro';
import { COOKIE_SESI, type User } from './api';

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
export function berandaUntuk(role: User['role']): string {
  switch (role) {
    case 'kasir':
      return '/kasir';
    case 'pengepak':
      return '/tiket';
    case 'owner':
      return '/produk';
  }
}
