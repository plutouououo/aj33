-- Jejak klik tombol dan menu dari pengguna sungguhan, untuk menimbang tata letak; dibaca lewat SQL, tidak ada yang bergantung pada isinya.
CREATE TABLE "jejak_ui" (
    "id" BIGSERIAL PRIMARY KEY,
    "user_id" UUID REFERENCES "users"("id") ON DELETE SET NULL,
    "halaman" VARCHAR(200) NOT NULL,
    "area" VARCHAR(50),
    "target" VARCHAR(200) NOT NULL,
    "dibuat_pada" TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX "jejak_ui_dibuat_pada_idx" ON "jejak_ui" ("dibuat_pada");

-- Defense-in-depth inert seperti tabel lain (lihat 0002 dan docs/panduan-deploy.md §3).
ALTER TABLE "jejak_ui" ENABLE ROW LEVEL SECURITY;
