/**
 * Penjaga sesi untuk seluruh halaman.
 *
 * Pemeriksaan dilakukan di satu tempat, bukan di tiap halaman, supaya
 * halaman baru tidak bisa lupa memasangnya. Hasilnya ditaruh di
 * `context.locals` agar halaman tidak perlu memanggil /auth/me lagi.
 */
import { defineMiddleware } from 'astro:middleware';
import { api, ApiRequestError, type User } from './lib/api';
import {
  ambilToken,
  berandaUntuk,
  bolehAkses,
  bolehTanpaLogin,
  hapusSesi,
  harusGantiPassword,
} from './lib/session';

export const onRequest = defineMiddleware(async (context, next) => {
  const { pathname } = context.url;

  // Aset statis dan service worker tidak lewat pemeriksaan sesi.
  if (pathname.startsWith('/_') || pathname.includes('.')) {
    return next();
  }

  const token = ambilToken(context.cookies);

  if (token) {
    try {
      context.locals.user = await api<User>('/auth/me', { token });
      context.locals.token = token;
    } catch (err) {
      // Token kedaluwarsa atau dicabut: buang cookie-nya supaya pengguna
      // tidak terjebak memantul antara halaman yang menganggapnya login
      // dan backend yang menolaknya.
      if (err instanceof ApiRequestError && err.status === 401) {
        hapusSesi(context.cookies);
      } else {
        throw err;
      }
    }
  }

  const user = context.locals.user;

  if (!user) {
    return bolehTanpaLogin(pathname) ? next() : context.redirect('/login', 302);
  }

  // Sebelum pemeriksaan peran: akun berpassword sementara tidak boleh
  // mengerjakan apa pun, termasuk halaman yang perannya memang berhak.
  if (harusGantiPassword(user, pathname)) {
    return context.redirect('/ganti-password', 302);
  }

  // Pembatasan peran ditegakkan DI SINI, bukan di tiap halaman. Menu yang
  // disembunyikan bukan penjaga -- sampai pemeriksaan ini ada, kasir yang
  // mengetik /produk di bilah alamat tetap mendapatkan halamannya, karena
  // backend membiarkan siapa pun yang sudah login membaca katalog.
  if (!bolehAkses(user.role, pathname)) {
    return context.redirect(berandaUntuk(user.role), 302);
  }

  return next();
});
