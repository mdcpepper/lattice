SET LOCAL lock_timeout = '5s';

CREATE TYPE PROMOTIONABLE_TYPE AS ENUM (
    'direct',
    'positional',
    'mix_and_match',
    'tiered_threshold'
);

CREATE TABLE promotions (
    uuid UUID PRIMARY KEY,

    tenant_uuid UUID NOT NULL
        DEFAULT NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid,

    promotionable_type PROMOTIONABLE_TYPE NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,

    CONSTRAINT promotions_tenant_uuid_uuid_uniq UNIQUE (tenant_uuid, uuid),

    CONSTRAINT promotions_tenant_fk
        FOREIGN KEY (tenant_uuid)
        REFERENCES tenants (uuid)
        ON DELETE CASCADE
);

CREATE INDEX promotions_promotionable_type_index
    ON promotions (promotionable_type);

CREATE POLICY promotions_tenant_select_policy ON promotions
    FOR SELECT
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY promotions_tenant_insert_policy ON promotions
    FOR INSERT
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY promotions_tenant_update_policy ON promotions
    FOR UPDATE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid)
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY promotions_tenant_delete_policy ON promotions
    FOR DELETE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

ALTER TABLE promotions
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY;

CREATE TRIGGER promotions_set_updated_at
    BEFORE UPDATE ON promotions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
