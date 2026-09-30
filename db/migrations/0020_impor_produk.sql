-- Impor produk massal dari spreadsheet (.xlsx/.csv), lewat staging.
--
-- Data TIDAK PERNAH masuk langsung ke `products` saat upload. Baris mentah
-- singgah di `import_rows` sampai Owner meninjau, menyetujui, dan proses
-- commit di background menuliskannya lewat jalur produk yang sama dengan
-- form manual (lihat `catalog::service`) -- bukan INSERT langsung.

CREATE TABLE "import_batches" (
    "id"                 UUID NOT NULL DEFAULT gen_random_uuid(),
    "file_name"          VARCHAR(255) NOT NULL,
    "file_sha256"        VARCHAR(64) NOT NULL,
    "status"             VARCHAR(20) NOT NULL DEFAULT 'draft',
    "total_rows"         INTEGER NOT NULL DEFAULT 0,
    -- Hanya boleh disetel ulang selama draft/pending_review -- ditegakkan
    -- `import::service`, bukan di sini.
    "publish_on_commit"  BOOLEAN NOT NULL DEFAULT true,
    "uploaded_by"        UUID,
    "uploaded_at"        TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "submitted_at"       TIMESTAMPTZ(6),
    "reviewed_by"        UUID,
    "reviewed_at"        TIMESTAMPTZ(6),
    "review_note"        VARCHAR(500),
    "committed_at"       TIMESTAMPTZ(6),
    "ok_count"           INTEGER NOT NULL DEFAULT 0,
    "fail_count"         INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT "import_batches_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "import_batches_status_check" CHECK ("status" IN
        ('draft', 'pending_review', 'approved', 'committing', 'committed', 'cancelled'))
);

ALTER TABLE "import_batches" ADD CONSTRAINT "import_batches_uploaded_by_fkey"
    FOREIGN KEY ("uploaded_by") REFERENCES "users"("id") ON DELETE SET NULL;
ALTER TABLE "import_batches" ADD CONSTRAINT "import_batches_reviewed_by_fkey"
    FOREIGN KEY ("reviewed_by") REFERENCES "users"("id") ON DELETE SET NULL;

CREATE TABLE "import_rows" (
    "id"                 UUID NOT NULL DEFAULT gen_random_uuid(),
    "batch_id"           UUID NOT NULL,
    "row_no"             INTEGER NOT NULL,
    -- Cuplikan sel asli, apa adanya -- dipakai menampilkan "yang sungguh
    -- ditulis di berkas" di panel review, terpisah dari kolom yang sudah
    -- diurai di bawah.
    "raw"                JSONB NOT NULL DEFAULT '{}',
    "name"               VARCHAR(220),
    "sku"                VARCHAR(50),
    "category_text"      VARCHAR(150),
    "brand_text"         VARCHAR(150),
    -- Dibutuhkan sku::rakit() untuk baris CREATE -- SKU produk di sini
    -- selalu rakitan otomatis, tidak pernah diketik manual.
    "product_type"       VARCHAR(150),
    "variant_grade"      VARCHAR(150),
    "variant_size"       VARCHAR(150),
    "category_id"        UUID,
    "lowest_price"       DECIMAL(14,2),
    "cost"               DECIMAL(14,2),
    "margin_pct"         DECIMAL(6,3),
    "stock"              INTEGER,
    "published"          BOOLEAN,
    "action"             VARCHAR(10) NOT NULL DEFAULT 'create',
    "match_product_id"   UUID,
    "issues"             JSONB NOT NULL DEFAULT '[]',
    "commit_state"       VARCHAR(10) NOT NULL DEFAULT 'pending',
    "commit_product_id"  UUID,
    "commit_error"       VARCHAR(500),

    CONSTRAINT "import_rows_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "import_rows_action_check" CHECK ("action" IN ('create', 'update', 'skip')),
    CONSTRAINT "import_rows_commit_state_check" CHECK ("commit_state" IN ('pending', 'ok', 'failed'))
);

ALTER TABLE "import_rows" ADD CONSTRAINT "import_rows_batch_id_fkey"
    FOREIGN KEY ("batch_id") REFERENCES "import_batches"("id") ON DELETE CASCADE;
ALTER TABLE "import_rows" ADD CONSTRAINT "import_rows_category_id_fkey"
    FOREIGN KEY ("category_id") REFERENCES "categories"("id") ON DELETE SET NULL;
ALTER TABLE "import_rows" ADD CONSTRAINT "import_rows_match_product_id_fkey"
    FOREIGN KEY ("match_product_id") REFERENCES "products"("id") ON DELETE SET NULL;
ALTER TABLE "import_rows" ADD CONSTRAINT "import_rows_commit_product_id_fkey"
    FOREIGN KEY ("commit_product_id") REFERENCES "products"("id") ON DELETE SET NULL;

-- Satu baris per nomor baris berkas per batch. TIDAK ADA indeks kedua di
-- atas (batch_id, row_no) -- ini SATU-SATUNYA, dipakai keduanya: unik dan
-- pencarian per batch.
CREATE UNIQUE INDEX "import_rows_batch_id_row_no_key"
    ON "import_rows" ("batch_id", "row_no");

CREATE TABLE "import_aliases" (
    "id"           UUID NOT NULL DEFAULT gen_random_uuid(),
    -- Hanya 'category': merek tidak punya tabel rujukan (lihat
    -- catalog/repo.rs -- `brand_name` adalah VARCHAR bebas tanpa FK), jadi
    -- tidak butuh alias.
    "kind"         VARCHAR(10) NOT NULL DEFAULT 'category',
    "alias_norm"   VARCHAR(150) NOT NULL,
    "target_id"    UUID,
    "created_by"   UUID,
    "hits"         INTEGER NOT NULL DEFAULT 0,
    "created_at"   TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "import_aliases_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "import_aliases_kind_check" CHECK ("kind" = 'category')
);

ALTER TABLE "import_aliases" ADD CONSTRAINT "import_aliases_target_id_fkey"
    FOREIGN KEY ("target_id") REFERENCES "categories"("id") ON DELETE SET NULL;
ALTER TABLE "import_aliases" ADD CONSTRAINT "import_aliases_created_by_fkey"
    FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE SET NULL;

CREATE UNIQUE INDEX "import_aliases_kind_alias_norm_key"
    ON "import_aliases" ("kind", "alias_norm");

-- Defense-in-depth inert seperti 30 tabel lain di 0002 -- satu role Postgres
-- memiliki semuanya, jadi tanpa policy ini tidak membatasi apa pun. Lihat
-- docs/panduan-deploy.md §3.
ALTER TABLE "import_batches" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "import_rows" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "import_aliases" ENABLE ROW LEVEL SECURITY;
