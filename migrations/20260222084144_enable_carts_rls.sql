SET LOCAL lock_timeout = '5s';

CREATE POLICY carts_tenant_select_policy ON carts
    FOR SELECT
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY carts_tenant_insert_policy ON carts
    FOR INSERT
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY carts_tenant_update_policy ON carts
    FOR UPDATE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid)
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY carts_tenant_delete_policy ON carts
    FOR DELETE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

ALTER TABLE carts
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY,
    ALTER COLUMN tenant_uuid
        SET DEFAULT NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid;
