-- Edit massal produk lewat xlsx: berkas yang diunggah diurai jadi rencana per baris dan singgah di sini sampai Owner menekan Terapkan.

CREATE TABLE "edit_massal" (
    "id"         UUID NOT NULL DEFAULT gen_random_uuid(),
    "kelompok"   VARCHAR(10) NOT NULL,
    "file_name"  VARCHAR(255) NOT NULL,
    "status"     VARCHAR(12) NOT NULL DEFAULT 'pratinjau',
    "baris"      JSONB NOT NULL DEFAULT '[]',
    "hasil"      JSONB,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "applied_at" TIMESTAMPTZ(6),

    CONSTRAINT "edit_massal_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "edit_massal_kelompok_check" CHECK ("kelompok" IN ('utama', 'stok')),
    CONSTRAINT "edit_massal_status_check" CHECK ("status" IN ('pratinjau', 'diterapkan'))
);

ALTER TABLE "edit_massal" ADD CONSTRAINT "edit_massal_created_by_fkey"
    FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE SET NULL;

ALTER TABLE "edit_massal" ENABLE ROW LEVEL SECURITY;
