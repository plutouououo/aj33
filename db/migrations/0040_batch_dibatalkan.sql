-- Pembatalan batch punya reason sendiri, bukan `restock` negatif, supaya ledger membedakan barang masuk yang ditarik kembali dari barang masuk biasa.

ALTER TABLE "stock_adjustments" DROP CONSTRAINT "stock_adjustments_reason_check";

ALTER TABLE "stock_adjustments"
    ADD CONSTRAINT "stock_adjustments_reason_check"
    CHECK (("reason")::text = ANY (ARRAY[
        'sale'::character varying,
        'manual_adjustment'::character varying,
        'void_reversal'::character varying,
        'external_order'::character varying,
        'restock'::character varying,
        'batch_dibatalkan'::character varying
    ]::text[]));

-- Baris lama yang dibatalkan sudah pasti `restock` negatif; dipindah setelah CHECK baru menerima nilainya.
UPDATE "stock_adjustments"
SET "reason" = 'batch_dibatalkan'
WHERE "reason" = 'restock' AND "change_qty" < 0;

-- Barang masuk selalu menambah dan pembatalan batch selalu mengurangi, jadi arah perubahan dijaga CHECK.
ALTER TABLE "stock_adjustments"
    ADD CONSTRAINT "stock_adjustments_batch_arah_check"
    CHECK (
        ("reason" = 'restock' AND "change_qty" > 0)
        OR ("reason" = 'batch_dibatalkan' AND "change_qty" < 0)
        OR "reason" NOT IN ('restock', 'batch_dibatalkan')
    );
