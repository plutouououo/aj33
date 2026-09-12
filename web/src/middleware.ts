/**
 * Penjaga sesi untuk seluruh halaman.
 *
 * Pemeriksaan dilakukan di satu tempat, bukan di tiap halaman, supaya
 * halaman baru tidak bisa lupa memasangnya. Hasilnya ditaruh di
 * `context.locals` agar halaman tidak perlu memanggil /auth/me lagi.
 */
import { defineMiddleware } from 'astro:middleware';
import { api, ApiRequestError, type User } from './lib/api';
import { ambilToken, bolehTanpaLogin, hapusSesi } from './lib/session';

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

  if (!context.locals.user && !bolehTanpaLogin(pathname)) {
    return context.redirect('/login', 302);
  }

  return next();
});
