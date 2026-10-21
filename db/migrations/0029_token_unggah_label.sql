-- Token unggah label untuk Shortcut iPhone, yang tak membawa cookie sesi; satu per pengguna, membuat baru menggantikan yang lama.
-- Yang disimpan hanya SHA-256 token: token acak 256 bit tak butuh hash lambat seperti password, dan bocornya tabel tak membocorkan token.
CREATE TABLE "label_upload_tokens" (
    "user_id" UUID PRIMARY KEY REFERENCES "users"("id") ON DELETE CASCADE,
    "token_hash" TEXT NOT NULL UNIQUE,
    "created_at" TIMESTAMPTZ NOT NULL DEFAULT now(),
    "last_used_at" TIMESTAMPTZ
);

-- Defense-in-depth inert seperti tabel lain (lihat 0002 dan docs/panduan-deploy.md §3).
ALTER TABLE "label_upload_tokens" ENABLE ROW LEVEL SECURITY;
