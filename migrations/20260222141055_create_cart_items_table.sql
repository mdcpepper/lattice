SET LOCAL lock_timeout = '5s';

CREATE TABLE cart_items (
    uuid uuid PRIMARY KEY,

    base_price  bigint NOT NULL,

    cart_uuid    uuid NOT NULL,
    product_uuid uuid NOT NULL,

    tenant_uuid uuid NOT NULL
        DEFAULT NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid,

    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz,

    CONSTRAINT cart_items_tenant_fk
        FOREIGN KEY (tenant_uuid)
        REFERENCES tenants (uuid)
        ON DELETE CASCADE,

    CONSTRAINT cart_items_cart_fk
        FOREIGN KEY (tenant_uuid, cart_uuid)
        REFERENCES carts (tenant_uuid, uuid)
        ON DELETE CASCADE,

    CONSTRAINT cart_items_product_fk
        FOREIGN KEY (tenant_uuid, product_uuid)
        REFERENCES products (tenant_uuid, uuid)
        ON DELETE CASCADE
);

CREATE INDEX cart_items_active_by_cart_idx ON cart_items (cart_uuid) WHERE deleted_at IS NULL;
CREATE INDEX cart_items_product_uuid_idx ON cart_items (product_uuid);

CREATE POLICY cart_items_tenant_select_policy ON cart_items
    FOR SELECT
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY cart_items_tenant_insert_policy ON cart_items
    FOR INSERT
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY cart_items_tenant_update_policy ON cart_items
    FOR UPDATE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid)
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY cart_items_tenant_delete_policy ON cart_items
    FOR DELETE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

ALTER TABLE cart_items
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY;
