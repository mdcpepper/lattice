SET LOCAL lock_timeout = '5s';

CREATE POLICY products_tenant_select_policy ON products
    FOR SELECT
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY products_tenant_insert_policy ON products
    FOR INSERT
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY products_tenant_update_policy ON products
    FOR UPDATE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid)
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY products_tenant_delete_policy ON products
    FOR DELETE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

ALTER TABLE products
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY,
    ALTER COLUMN tenant_uuid
        SET DEFAULT NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid;
