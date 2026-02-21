SET LOCAL lock_timeout = '5s';

CREATE POLICY product_details_tenant_select_policy ON product_details
    FOR SELECT
    USING (
        EXISTS (
            SELECT 1
            FROM products
            WHERE products.uuid = product_details.product_uuid
              AND products.tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid
        )
    );

CREATE POLICY product_details_tenant_insert_policy ON product_details
    FOR INSERT
    WITH CHECK (
        EXISTS (
            SELECT 1
            FROM products
            WHERE products.uuid = product_details.product_uuid
              AND products.tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid
              AND products.deleted_at IS NULL
        )
    );

CREATE POLICY product_details_tenant_update_policy ON product_details
    FOR UPDATE
    USING (
        EXISTS (
            SELECT 1
            FROM products
            WHERE products.uuid = product_details.product_uuid
              AND products.tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid
              AND products.deleted_at IS NULL
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1
            FROM products
            WHERE products.uuid = product_details.product_uuid
              AND products.tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid
              AND products.deleted_at IS NULL
        )
    );

CREATE POLICY product_details_tenant_delete_policy ON product_details
    FOR DELETE
    USING (
        EXISTS (
            SELECT 1
            FROM products
            WHERE products.uuid = product_details.product_uuid
              AND products.tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid
        )
    );

ALTER TABLE product_details
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY;
