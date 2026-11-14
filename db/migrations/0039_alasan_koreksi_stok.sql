-- Alasan pengurangan stok manual: rusak, kedaluwarsa, dan hilang menjadi kerugian di laba rugi, salah_catat hanya membetulkan angka; uraian di docs/status-proyek.md.
ALTER TABLE "stock_adjustments" ADD COLUMN "manual_reason" VARCHAR(20);

-- Hanya pengurangan manual yang boleh beralasan, supaya kerugian tak bisa menempel pada penjualan, void, atau barang masuk.
ALTER TABLE "stock_adjustments"
    ADD CONSTRAINT "stock_adjustments_manual_reason_check"
    CHECK (
        manual_reason IS NULL
        OR (
            reason = 'manual_adjustment'
            AND change_qty < 0
            AND manual_reason IN ('rusak', 'kedaluwarsa', 'hilang', 'salah_catat')
        )
    );
