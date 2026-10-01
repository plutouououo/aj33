/** Penjaga sesi untuk semua halaman di satu tempat (halaman baru tak bisa lupa memasangnya); hasilnya di `context.locals` agar tak memanggil /auth/me lagi. */
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

const ASET_STATIS = /\.(?:png|svg|ico|webmanifest|js)$/;

export const onRequest = defineMiddleware(async (context, next) => {
  const { pathname } = context.url;

  // Aset statis dikenali dari akhiran berkas di `public/`, bukan "ada titik" (id seperti /produk/a.b lolos dan 500); CSV sengaja lewat middleware, dan foto produk (dari Azure) tetap butuh login.
  const foto = pathname.startsWith('/foto/');
  if (!foto && (pathname.startsWith('/_') || ASET_STATIS.test(pathname))) {
    return next();
  }

  const token = ambilToken(context.cookies);

  if (token) {
    try {
      context.locals.user = await api<User>('/auth/me', { token });
      context.locals.token = token;
    } catch (err) {
      // Token kedaluwarsa atau dicabut: buang cookie agar pengguna tidak memantul antara halaman dan backend yang menolaknya.
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

  // Sebelum cek peran: akun berpassword sementara tidak boleh mengerjakan apa pun, termasuk halaman yang perannya berhak.
  if (harusGantiPassword(user, pathname)) {
    return context.redirect('/pengaturan/akun', 302);
  }

  // Pembatasan peran ditegakkan di sini, bukan di tiap halaman, karena menu yang disembunyikan bukan penjaga dan backend membiarkan siapa pun yang login membaca katalog.
  if (!bolehAkses(user.role, pathname)) {
    return context.redirect(berandaUntuk(user.role), 302);
  }

  return next();
});
