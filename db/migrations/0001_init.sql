CREATE SCHEMA IF NOT EXISTS "public";

CREATE TABLE "categories" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "name" VARCHAR(150) NOT NULL,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "categories_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "channel_attribute_def_values" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "attribute_def_id" UUID NOT NULL,
    "value_id" VARCHAR(100) NOT NULL,
    "value_name" VARCHAR(200),
    "value_unit" VARCHAR(50),

    CONSTRAINT "channel_attribute_def_values_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "channel_attribute_defs" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_id" UUID NOT NULL,
    "external_category_id" VARCHAR(100) NOT NULL,
    "attribute_id" VARCHAR(100) NOT NULL,
    "name" VARCHAR(200),
    "is_mandatory" BOOLEAN NOT NULL DEFAULT false,
    "input_type" SMALLINT,
    "validation_meta" JSONB,
    "cached_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "channel_attribute_defs_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "channel_categories" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_id" UUID NOT NULL,
    "external_category_id" VARCHAR(100) NOT NULL,
    "parent_external_id" VARCHAR(100),
    "name_default" VARCHAR(200),
    "name_display" VARCHAR(200),
    "is_leaf" BOOLEAN NOT NULL DEFAULT false,
    "cached_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "channel_categories_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "channel_listings" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_id" UUID NOT NULL,
    "product_id" UUID,
    "external_item_id" VARCHAR(100) NOT NULL,
    "external_sku" VARCHAR(100),
    "external_category_id" VARCHAR(100),
    "channel_status" VARCHAR(20) NOT NULL DEFAULT 'NORMAL',
    "sync_status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "sync_error_message" TEXT,
    "has_model" BOOLEAN NOT NULL DEFAULT false,
    "is_fulfilled_by_platform" BOOLEAN NOT NULL DEFAULT false,
    "total_reserved_stock" INTEGER,
    "total_available_stock" INTEGER,
    "description_type" VARCHAR(20) DEFAULT 'normal',
    "scheduled_publish_time" TIMESTAMPTZ(6),
    "video_upload_id" VARCHAR(100),
    "last_synced_at" TIMESTAMPTZ(6),
    "last_updated_at_channel" TIMESTAMPTZ(6),
    "raw_payload" JSONB,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "channel_listings_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "channel_status_mapping" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_id" UUID NOT NULL,
    "external_status" VARCHAR(50) NOT NULL,
    "internal_status" VARCHAR(20) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "channel_status_mapping_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "customers" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "name" VARCHAR(150),
    "phone" VARCHAR(30),
    "email" VARCHAR(150),
    "source" VARCHAR(30) NOT NULL DEFAULT 'walk_in',
    "external_customer_ref" VARCHAR(100),
    "external_username" VARCHAR(150),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "customers_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "expenses" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "category" VARCHAR(100),
    "amount" DECIMAL(14,2) NOT NULL,
    "description" TEXT,
    "expense_date" DATE NOT NULL,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "expenses_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "external_order_items" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "external_order_id" UUID NOT NULL,
    "product_id" UUID,
    "external_item_ref" VARCHAR(100),
    "item_name_snapshot" VARCHAR(200) NOT NULL,
    "qty" INTEGER NOT NULL,
    "unit_price" DECIMAL(14,2),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "order_item_id" VARCHAR(100),
    "model_id" VARCHAR(50) DEFAULT '0',
    "model_name" VARCHAR(150),
    "model_sku" VARCHAR(100),
    "unit_price_original" DECIMAL(14,2),
    "qty_active" INTEGER,
    "qty_cancelled" INTEGER NOT NULL DEFAULT 0,
    "qty_returned" INTEGER NOT NULL DEFAULT 0,
    "weight" DECIMAL(8,3),
    "is_wholesale" BOOLEAN NOT NULL DEFAULT false,
    "image_url" VARCHAR(500),

    CONSTRAINT "external_order_items_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "external_orders" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_id" UUID NOT NULL,
    "external_order_id" VARCHAR(100) NOT NULL,
    "customer_id" UUID,
    "status" VARCHAR(20) NOT NULL DEFAULT 'new',
    "sla_type" VARCHAR(20) NOT NULL,
    "sla_deadline" TIMESTAMPTZ(6),
    "total_amount" DECIMAL(14,2),
    "payment_method" VARCHAR(50),
    "raw_payload" JSONB,
    "received_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "external_status_raw" VARCHAR(50),
    "fulfillment_flag" VARCHAR(30),
    "is_cod" BOOLEAN NOT NULL DEFAULT false,
    "shipping_carrier" VARCHAR(100),
    "currency" VARCHAR(3) NOT NULL DEFAULT 'IDR',
    "paid_at" TIMESTAMPTZ(6),
    "days_to_ship" INTEGER,
    "cancel_by" VARCHAR(20),
    "cancel_reason" TEXT,
    "buyer_message" TEXT,
    "seller_note" TEXT,
    "dropshipper_name" VARCHAR(150),
    "dropshipper_phone" VARCHAR(30),
    "pickup_done_at" TIMESTAMPTZ(6),
    "warnings" JSONB,

    CONSTRAINT "external_orders_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "notifications" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "user_id" UUID NOT NULL,
    "type" VARCHAR(50) NOT NULL,
    "title" VARCHAR(200) NOT NULL,
    "message" TEXT,
    "reference_type" VARCHAR(20),
    "reference_id" UUID,
    "is_read" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "notifications_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "order_packages" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "external_order_id" UUID NOT NULL,
    "package_number" VARCHAR(100) NOT NULL,
    "logistics_status" VARCHAR(50),
    "logistics_channel_id" VARCHAR(50),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "order_packages_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "order_shipping_address" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "external_order_id" UUID NOT NULL,
    "recipient_name" VARCHAR(150),
    "phone" VARCHAR(30),
    "full_address" TEXT,
    "city" VARCHAR(100),
    "district" VARCHAR(100),
    "town" VARCHAR(100),
    "state" VARCHAR(100),
    "zipcode" VARCHAR(20),
    "latitude" DECIMAL(10,6),
    "longitude" DECIMAL(10,6),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "order_shipping_address_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "platforms" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "platform_name" VARCHAR(30) NOT NULL,
    "shop_id_external" VARCHAR(100),
    "access_token_encrypted" TEXT,
    "refresh_token_encrypted" TEXT,
    "token_expires_at" TIMESTAMPTZ(6),
    "is_connected" BOOLEAN NOT NULL DEFAULT false,
    "last_synced_at" TIMESTAMPTZ(6),
    "last_sync_status" VARCHAR(10),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "platforms_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_batches" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "product_id" UUID NOT NULL,
    "batch_number" VARCHAR(100),
    "quantity" INTEGER NOT NULL,
    "expiry_date" DATE,
    "received_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "created_by" UUID,

    CONSTRAINT "product_batches_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_channel_attribute_values" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "product_channel_attribute_id" UUID NOT NULL,
    "value_id" VARCHAR(100),
    "value_name" VARCHAR(200),
    "value_unit" VARCHAR(50),

    CONSTRAINT "product_channel_attribute_values_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_channel_attributes" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "channel_listing_id" UUID NOT NULL,
    "attribute_id" VARCHAR(100) NOT NULL,
    "is_mandatory" BOOLEAN NOT NULL DEFAULT false,

    CONSTRAINT "product_channel_attributes_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_channel_logistics" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "channel_listing_id" UUID NOT NULL,
    "logistic_id" VARCHAR(50) NOT NULL,
    "logistic_name" VARCHAR(150),
    "enabled" BOOLEAN NOT NULL DEFAULT true,
    "is_free" BOOLEAN NOT NULL DEFAULT false,
    "shipping_fee" DECIMAL(14,2),
    "size_id" VARCHAR(50),
    "estimated_shipping_fee" DECIMAL(14,2),

    CONSTRAINT "product_channel_logistics_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_images" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "channel_listing_id" UUID NOT NULL,
    "image_id" VARCHAR(150) NOT NULL,
    "image_url" VARCHAR(500),
    "sort_order" INTEGER NOT NULL DEFAULT 0,
    "ratio" VARCHAR(10),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "product_images_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_stock_locations" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "channel_listing_id" UUID NOT NULL,
    "location_id" VARCHAR(100),
    "stock" INTEGER NOT NULL DEFAULT 0,
    "if_saleable" BOOLEAN NOT NULL DEFAULT true,
    "platform_held_stock" INTEGER,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "product_stock_locations_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "product_wholesale_tiers" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "channel_listing_id" UUID NOT NULL,
    "min_count" INTEGER NOT NULL,
    "max_count" INTEGER NOT NULL,
    "unit_price" DECIMAL(14,2) NOT NULL,

    CONSTRAINT "product_wholesale_tiers_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "products" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "category_id" UUID,
    "name" VARCHAR(200) NOT NULL,
    "sku" VARCHAR(100),
    "price" DECIMAL(14,2) NOT NULL,
    "cost_price" DECIMAL(14,2),
    "stock_qty" INTEGER NOT NULL DEFAULT 0,
    "low_stock_threshold" INTEGER NOT NULL DEFAULT 5,
    "image_url" VARCHAR(500),
    "unit" VARCHAR(30),
    "is_active" BOOLEAN NOT NULL DEFAULT true,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "gtin_code" VARCHAR(50),
    "brand_id" VARCHAR(50),
    "brand_name" VARCHAR(150),
    "condition" VARCHAR(10) DEFAULT 'NEW',
    "is_dangerous" BOOLEAN NOT NULL DEFAULT false,
    "is_preorder" BOOLEAN NOT NULL DEFAULT false,
    "days_to_ship" INTEGER,
    "min_purchase_limit" INTEGER,
    "max_purchase_limit" INTEGER,
    "weight_kg" DECIMAL(8,3),
    "package_length_cm" DECIMAL(8,2),
    "package_width_cm" DECIMAL(8,2),
    "package_height_cm" DECIMAL(8,2),

    CONSTRAINT "products_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "shopping_list_items" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "product_id" UUID NOT NULL,
    "suggested_qty" INTEGER NOT NULL,
    "status" VARCHAR(20) NOT NULL DEFAULT 'pending',
    "ordered_at" TIMESTAMPTZ(6),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "shopping_list_items_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "stock_adjustments" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "product_id" UUID NOT NULL,
    "change_qty" INTEGER NOT NULL,
    "reason" VARCHAR(30) NOT NULL,
    "reference_type" VARCHAR(20),
    "reference_id" UUID,
    "stock_before" INTEGER NOT NULL,
    "stock_after" INTEGER NOT NULL,
    "adjusted_by_user_id" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "stock_adjustments_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "store_settings" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "business_name" VARCHAR(200) NOT NULL,
    "address" TEXT,
    "phone" VARCHAR(30),
    "receipt_footer_note" TEXT,
    "logo_url" VARCHAR(500),
    "updated_by" UUID,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "store_settings_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "ticket_items" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "ticket_id" UUID NOT NULL,
    "product_id" UUID NOT NULL,
    "product_name_snapshot" VARCHAR(200) NOT NULL,
    "qty" INTEGER NOT NULL,
    "is_packed" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "ticket_items_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "tickets" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "external_order_id" UUID NOT NULL,
    "assigned_to_user_id" UUID,
    "status" VARCHAR(20) NOT NULL DEFAULT 'unassigned',
    "assigned_at" TIMESTAMPTZ(6),
    "assigned_by" UUID,
    "completed_at" TIMESTAMPTZ(6),
    "notes" TEXT,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "tickets_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "transaction_items" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "transaction_id" UUID NOT NULL,
    "product_id" UUID NOT NULL,
    "product_name_snapshot" VARCHAR(200) NOT NULL,
    "qty" INTEGER NOT NULL,
    "unit_price" DECIMAL(14,2) NOT NULL,
    "subtotal" DECIMAL(14,2) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "transaction_items_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "transactions" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "idempotency_key" VARCHAR(100) NOT NULL,
    "type" VARCHAR(20) NOT NULL,
    "customer_id" UUID,
    "cashier_user_id" UUID NOT NULL,
    "payment_method" VARCHAR(20) NOT NULL,
    "subtotal" DECIMAL(14,2) NOT NULL,
    "total_amount" DECIMAL(14,2) NOT NULL,
    "amount_paid" DECIMAL(14,2),
    "change_amount" DECIMAL(14,2),
    "status" VARCHAR(20) NOT NULL DEFAULT 'completed',
    "voided_at" TIMESTAMPTZ(6),
    "voided_by" UUID,
    "void_reason" TEXT,
    "synced_offline" BOOLEAN NOT NULL DEFAULT false,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "transactions_pkey" PRIMARY KEY ("id")
);

CREATE TABLE "users" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "name" VARCHAR(150) NOT NULL,
    "email_or_username" VARCHAR(150) NOT NULL,
    "password_hash" VARCHAR(255) NOT NULL,
    "role" VARCHAR(20) NOT NULL,
    "phone" VARCHAR(30),
    "is_active" BOOLEAN NOT NULL DEFAULT true,
    "created_by" UUID,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "users_pkey" PRIMARY KEY ("id")
);

CREATE UNIQUE INDEX "categories_name_key" ON "categories"("name");

CREATE INDEX "idx_channel_attribute_def_values_def" ON "channel_attribute_def_values"("attribute_def_id");

CREATE UNIQUE INDEX "idx_channel_attribute_defs_cat_attr" ON "channel_attribute_defs"("platform_id", "external_category_id", "attribute_id");

CREATE UNIQUE INDEX "idx_channel_categories_platform_extid" ON "channel_categories"("platform_id", "external_category_id");

CREATE INDEX "idx_channel_listings_product" ON "channel_listings"("product_id", "platform_id");

CREATE UNIQUE INDEX "idx_channel_listings_platform_extid" ON "channel_listings"("platform_id", "external_item_id");

CREATE UNIQUE INDEX "idx_channel_status_mapping_platform_status" ON "channel_status_mapping"("platform_id", "external_status");

CREATE INDEX "idx_customers_phone" ON "customers"("phone");

CREATE UNIQUE INDEX "idx_external_order_items_order_line" ON "external_order_items"("external_order_id", "order_item_id") WHERE (order_item_id IS NOT NULL);

CREATE INDEX "idx_external_orders_status_deadline" ON "external_orders"("status", "sla_deadline");

CREATE UNIQUE INDEX "idx_external_orders_platform_extid" ON "external_orders"("platform_id", "external_order_id");

CREATE INDEX "idx_notifications_user_unread" ON "notifications"("user_id", "is_read");

CREATE UNIQUE INDEX "idx_order_packages_order_number" ON "order_packages"("external_order_id", "package_number");

CREATE UNIQUE INDEX "idx_order_shipping_address_order" ON "order_shipping_address"("external_order_id");

CREATE INDEX "idx_product_batches_expiry" ON "product_batches"("expiry_date");

CREATE INDEX "idx_product_batches_product" ON "product_batches"("product_id");

CREATE INDEX "idx_product_channel_attributes_listing" ON "product_channel_attributes"("channel_listing_id");

CREATE INDEX "idx_product_channel_logistics_listing" ON "product_channel_logistics"("channel_listing_id");

CREATE INDEX "idx_product_images_listing" ON "product_images"("channel_listing_id");

CREATE INDEX "idx_product_stock_locations_listing" ON "product_stock_locations"("channel_listing_id");

CREATE INDEX "idx_product_wholesale_tiers_listing" ON "product_wholesale_tiers"("channel_listing_id");

CREATE UNIQUE INDEX "products_sku_key" ON "products"("sku");

CREATE INDEX "idx_products_low_stock" ON "products"("id") WHERE (stock_qty <= low_stock_threshold);

CREATE INDEX "idx_tickets_assignee_status" ON "tickets"("assigned_to_user_id", "status");

CREATE INDEX "idx_transaction_items_product_id" ON "transaction_items"("product_id");

CREATE UNIQUE INDEX "transactions_idempotency_key_key" ON "transactions"("idempotency_key");

CREATE INDEX "idx_transactions_created_at" ON "transactions"("created_at");

CREATE UNIQUE INDEX "users_email_or_username_key" ON "users"("email_or_username");

CREATE INDEX "idx_users_role" ON "users"("role");

ALTER TABLE "categories" ADD CONSTRAINT "categories_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_attribute_def_values" ADD CONSTRAINT "channel_attribute_def_values_attribute_def_id_fkey" FOREIGN KEY ("attribute_def_id") REFERENCES "channel_attribute_defs"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_attribute_defs" ADD CONSTRAINT "channel_attribute_defs_platform_id_fkey" FOREIGN KEY ("platform_id") REFERENCES "platforms"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_categories" ADD CONSTRAINT "channel_categories_platform_id_fkey" FOREIGN KEY ("platform_id") REFERENCES "platforms"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_listings" ADD CONSTRAINT "channel_listings_platform_id_fkey" FOREIGN KEY ("platform_id") REFERENCES "platforms"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_listings" ADD CONSTRAINT "channel_listings_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "channel_status_mapping" ADD CONSTRAINT "channel_status_mapping_platform_id_fkey" FOREIGN KEY ("platform_id") REFERENCES "platforms"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "expenses" ADD CONSTRAINT "expenses_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "external_order_items" ADD CONSTRAINT "external_order_items_external_order_id_fkey" FOREIGN KEY ("external_order_id") REFERENCES "external_orders"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "external_order_items" ADD CONSTRAINT "external_order_items_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "external_orders" ADD CONSTRAINT "external_orders_customer_id_fkey" FOREIGN KEY ("customer_id") REFERENCES "customers"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "external_orders" ADD CONSTRAINT "external_orders_platform_id_fkey" FOREIGN KEY ("platform_id") REFERENCES "platforms"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "notifications" ADD CONSTRAINT "notifications_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "order_packages" ADD CONSTRAINT "order_packages_external_order_id_fkey" FOREIGN KEY ("external_order_id") REFERENCES "external_orders"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "order_shipping_address" ADD CONSTRAINT "order_shipping_address_external_order_id_fkey" FOREIGN KEY ("external_order_id") REFERENCES "external_orders"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_batches" ADD CONSTRAINT "product_batches_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_batches" ADD CONSTRAINT "product_batches_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_channel_attribute_values" ADD CONSTRAINT "product_channel_attribute_val_product_channel_attribute_id_fkey" FOREIGN KEY ("product_channel_attribute_id") REFERENCES "product_channel_attributes"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_channel_attributes" ADD CONSTRAINT "product_channel_attributes_channel_listing_id_fkey" FOREIGN KEY ("channel_listing_id") REFERENCES "channel_listings"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_channel_logistics" ADD CONSTRAINT "product_channel_logistics_channel_listing_id_fkey" FOREIGN KEY ("channel_listing_id") REFERENCES "channel_listings"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_images" ADD CONSTRAINT "product_images_channel_listing_id_fkey" FOREIGN KEY ("channel_listing_id") REFERENCES "channel_listings"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_stock_locations" ADD CONSTRAINT "product_stock_locations_channel_listing_id_fkey" FOREIGN KEY ("channel_listing_id") REFERENCES "channel_listings"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "product_wholesale_tiers" ADD CONSTRAINT "product_wholesale_tiers_channel_listing_id_fkey" FOREIGN KEY ("channel_listing_id") REFERENCES "channel_listings"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "products" ADD CONSTRAINT "products_category_id_fkey" FOREIGN KEY ("category_id") REFERENCES "categories"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "products" ADD CONSTRAINT "products_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "shopping_list_items" ADD CONSTRAINT "shopping_list_items_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "stock_adjustments" ADD CONSTRAINT "stock_adjustments_adjusted_by_user_id_fkey" FOREIGN KEY ("adjusted_by_user_id") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "stock_adjustments" ADD CONSTRAINT "stock_adjustments_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "store_settings" ADD CONSTRAINT "store_settings_updated_by_fkey" FOREIGN KEY ("updated_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "ticket_items" ADD CONSTRAINT "ticket_items_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "ticket_items" ADD CONSTRAINT "ticket_items_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "tickets"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "tickets" ADD CONSTRAINT "tickets_assigned_by_fkey" FOREIGN KEY ("assigned_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "tickets" ADD CONSTRAINT "tickets_assigned_to_user_id_fkey" FOREIGN KEY ("assigned_to_user_id") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "tickets" ADD CONSTRAINT "tickets_external_order_id_fkey" FOREIGN KEY ("external_order_id") REFERENCES "external_orders"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "transaction_items" ADD CONSTRAINT "transaction_items_product_id_fkey" FOREIGN KEY ("product_id") REFERENCES "products"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "transaction_items" ADD CONSTRAINT "transaction_items_transaction_id_fkey" FOREIGN KEY ("transaction_id") REFERENCES "transactions"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "transactions" ADD CONSTRAINT "transactions_cashier_user_id_fkey" FOREIGN KEY ("cashier_user_id") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "transactions" ADD CONSTRAINT "transactions_customer_id_fkey" FOREIGN KEY ("customer_id") REFERENCES "customers"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "transactions" ADD CONSTRAINT "transactions_voided_by_fkey" FOREIGN KEY ("voided_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

ALTER TABLE "users" ADD CONSTRAINT "users_created_by_fkey" FOREIGN KEY ("created_by") REFERENCES "users"("id") ON DELETE NO ACTION ON UPDATE NO ACTION;

