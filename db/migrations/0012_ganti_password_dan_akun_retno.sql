-- Password sementara, dan akun kasir `retno`.
--
-- Belum ada halaman kelola pengguna, jadi satu-satunya cara memasang akun di
-- produksi adalah lewat migrasi. Itu berarti hash password-nya ikut ke dalam
-- repositori -- dan karena passwordnya sederhana (lihat catatan di bawah),
-- hash itu bisa ditebak siapa pun yang melihat git.
--
-- Kolom `must_change_password` adalah penutup lubang itu: akun yang
-- dipasang dari sini tidak bisa mengerjakan apa pun sebelum passwordnya
-- diganti. Selama nilainya true, middleware frontend memantulkan pengguna
-- ke /ganti-password dari halaman mana pun. Jadi password yang tertulis di
-- git ini hanya berlaku untuk satu hal: login pertama yang langsung
-- memintanya diganti.

-- Baku false. Akun yang sudah ada -- termasuk owner -- tidak sedang memakai
-- password sementara, dan menyalakannya untuk mereka akan mengunci seluruh
-- pengguna yang sekarang bisa bekerja.
ALTER TABLE "users"
    ADD COLUMN "must_change_password" BOOLEAN NOT NULL DEFAULT false;

-- Password awal: retno123. Sengaja mudah diucapkan lewat telepon, karena
-- umurnya memang hanya sampai login pertama.
--
-- ON CONFLICT DO NOTHING membuat migrasi ini aman dijalankan ulang, dan --
-- yang lebih penting -- tidak menimpa password yang sudah diganti Retno
-- seandainya baris ini dijalankan lagi di kemudian hari.
INSERT INTO users (name, email_or_username, password_hash, role, is_active, must_change_password)
VALUES (
    'Retno',
    'retno',
    '$2a$10$pV0lg593Rkt.GzzqOjui5O04aW04vEdJ72WbZAOfdh61tlqAHHGj.',
    'kasir',
    true,
    true
)
ON CONFLICT (email_or_username) DO NOTHING;
