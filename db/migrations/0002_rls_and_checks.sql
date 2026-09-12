-- Menyalakan Row Level Security dan memasang CHECK constraint.
--
-- Isi file ini direkonstruksi dari database dev pada 2026-09-04: baris
-- 1_rls_and_checks sudah tercatat di _prisma_migrations, tapi foldernya
-- tidak pernah ikut ter-commit. Tanpa file ini `prisma migrate deploy`
-- di database baru (mis. CI) menghasilkan schema tanpa satu pun CHECK,
-- jadi lebih longgar daripada production.
--
-- Di dev, RLS juga menyala di _prisma_migrations (efek linter Supabase).
-- Itu tabel bookkeeping milik Prisma, bukan milik aplikasi, jadi sengaja
-- TIDAK diikutkan di sini.

-- CreateCheckConstraints
ALTER TABLE "channel_listings" ADD CONSTRAINT "channel_listings_channel_status_check" CHECK (((channel_status)::text = ANY ((ARRAY['NORMAL'::character varying, 'UNLIST'::character varying, 'BANNED'::character varying, 'REVIEWING'::character varying, 'SELLER_DELETE'::character varying, 'SHOPEE_DELETE'::character varying])::text[])));
ALTER TABLE "channel_listings" ADD CONSTRAINT "channel_listings_description_type_check" CHECK (((description_type)::text = ANY ((ARRAY['normal'::character varying, 'extended'::character varying])::text[])));
ALTER TABLE "channel_listings" ADD CONSTRAINT "channel_listings_sync_status_check" CHECK (((sync_status)::text = ANY ((ARRAY['pending'::character varying, 'synced'::character varying, 'failed'::character varying])::text[])));

ALTER TABLE "channel_status_mapping" ADD CONSTRAINT "channel_status_mapping_internal_status_check" CHECK (((internal_status)::text = ANY ((ARRAY['new'::character varying, 'processing'::character varying, 'shipped'::character varying, 'completed'::character varying, 'cancelled'::character varying])::text[])));

ALTER TABLE "external_orders" ADD CONSTRAINT "external_orders_fulfillment_flag_check" CHECK (((fulfillment_flag)::text = ANY ((ARRAY['fulfilled_by_shopee'::character varying, 'fulfilled_by_cb_seller'::character varying, 'fulfilled_by_local_seller'::character varying, 'fulfilled_by_tiktok'::character varying])::text[])));
ALTER TABLE "external_orders" ADD CONSTRAINT "external_orders_sla_type_check" CHECK (((sla_type)::text = ANY ((ARRAY['instant'::character varying, 'same_day'::character varying, 'reguler'::character varying])::text[])));
ALTER TABLE "external_orders" ADD CONSTRAINT "external_orders_status_check" CHECK (((status)::text = ANY ((ARRAY['new'::character varying, 'processing'::character varying, 'shipped'::character varying, 'completed'::character varying, 'cancelled'::character varying])::text[])));

ALTER TABLE "notifications" ADD CONSTRAINT "notifications_reference_type_check" CHECK (((reference_type)::text = ANY ((ARRAY['external_order'::character varying, 'ticket'::character varying])::text[])));

ALTER TABLE "platforms" ADD CONSTRAINT "platforms_last_sync_status_check" CHECK (((last_sync_status)::text = ANY ((ARRAY['success'::character varying, 'failed'::character varying])::text[])));
ALTER TABLE "platforms" ADD CONSTRAINT "platforms_platform_name_check" CHECK (((platform_name)::text = ANY ((ARRAY['shopee'::character varying, 'tokopedia'::character varying, 'tiktok'::character varying, 'fakestore'::character varying])::text[])));

ALTER TABLE "products" ADD CONSTRAINT "products_condition_check" CHECK (((condition)::text = ANY ((ARRAY['NEW'::character varying, 'USED'::character varying])::text[])));

ALTER TABLE "shopping_list_items" ADD CONSTRAINT "shopping_list_items_status_check" CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'ordered'::character varying])::text[])));

ALTER TABLE "stock_adjustments" ADD CONSTRAINT "stock_adjustments_reason_check" CHECK (((reason)::text = ANY ((ARRAY['sale'::character varying, 'manual_adjustment'::character varying, 'void_reversal'::character varying, 'external_order'::character varying, 'restock'::character varying])::text[])));
ALTER TABLE "stock_adjustments" ADD CONSTRAINT "stock_adjustments_reference_type_check" CHECK (((reference_type)::text = ANY ((ARRAY['transaction'::character varying, 'external_order'::character varying, 'manual'::character varying])::text[])));

ALTER TABLE "tickets" ADD CONSTRAINT "tickets_status_check" CHECK (((status)::text = ANY ((ARRAY['unassigned'::character varying, 'assigned'::character varying, 'packing'::character varying, 'packed'::character varying, 'handed_over'::character varying])::text[])));

ALTER TABLE "transactions" ADD CONSTRAINT "transactions_payment_method_check" CHECK (((payment_method)::text = ANY ((ARRAY['cash'::character varying, 'transfer'::character varying, 'ewallet'::character varying])::text[])));
ALTER TABLE "transactions" ADD CONSTRAINT "transactions_status_check" CHECK (((status)::text = ANY ((ARRAY['completed'::character varying, 'voided'::character varying])::text[])));
ALTER TABLE "transactions" ADD CONSTRAINT "transactions_type_check" CHECK (((type)::text = ANY ((ARRAY['walk_in'::character varying, 'pre_order'::character varying])::text[])));

ALTER TABLE "users" ADD CONSTRAINT "users_role_check" CHECK (((role)::text = ANY ((ARRAY['owner'::character varying, 'kasir'::character varying, 'pengepak'::character varying])::text[])));

-- EnableRowLevelSecurity

ALTER TABLE "categories" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "channel_attribute_def_values" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "channel_attribute_defs" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "channel_categories" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "channel_listings" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "channel_status_mapping" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "customers" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "expenses" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "external_order_items" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "external_orders" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "notifications" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "order_packages" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "order_shipping_address" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "platforms" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_batches" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_channel_attribute_values" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_channel_attributes" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_channel_logistics" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_images" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_stock_locations" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "product_wholesale_tiers" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "products" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "shopping_list_items" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "stock_adjustments" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "store_settings" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "ticket_items" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "tickets" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "transaction_items" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "transactions" ENABLE ROW LEVEL SECURITY;
ALTER TABLE "users" ENABLE ROW LEVEL SECURITY;
