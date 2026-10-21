-- Akun pengepak `pengepak`, dipasang lewat migrasi seperti `retno` di 0012 karena belum ada halaman kelola pengguna.
-- Password awal: pengepak123; must_change_password memaksa diganti saat login pertama, jadi hash di git ini hanya berlaku sekali.
-- ON CONFLICT DO NOTHING agar tak menimpa password yang sudah diganti, dan tak bentrok dengan akun `pengepak` dari seed dev.
INSERT INTO users (name, email_or_username, password_hash, role, is_active, must_change_password)
VALUES (
    'Pengepak',
    'pengepak',
    '$2b$10$nz/ylxU.bgZBILF5fMXTL.pKDeg6JybBm8JwcaNfircuraBjcAOC6',
    'pengepak',
    true,
    true
)
ON CONFLICT (email_or_username) DO NOTHING;
