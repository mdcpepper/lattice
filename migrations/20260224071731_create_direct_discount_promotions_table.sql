SET LOCAL lock_timeout = '5s';

CREATE TABLE direct_discount_promotions (
    uuid UUID PRIMARY KEY,

    tenant_uuid UUID NOT NULL
        DEFAULT NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,

    CONSTRAINT direct_discount_promotions_tenant_uuid_uuid_uniq UNIQUE (tenant_uuid, uuid),

    CONSTRAINT direct_discount_promotions_tenant_fk
        FOREIGN KEY (tenant_uuid)
        REFERENCES tenants (uuid)
        ON DELETE CASCADE
);

CREATE POLICY direct_discount_promotions_tenant_select_policy ON direct_discount_promotions
    FOR SELECT
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY direct_discount_promotions_tenant_insert_policy ON direct_discount_promotions
    FOR INSERT
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY direct_discount_promotions_tenant_update_policy ON direct_discount_promotions
    FOR UPDATE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid)
    WITH CHECK (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

CREATE POLICY direct_discount_promotions_tenant_delete_policy ON direct_discount_promotions
    FOR DELETE
    USING (tenant_uuid = NULLIF(current_setting('app.current_tenant_uuid', true), '')::uuid);

ALTER TABLE direct_discount_promotions
    ENABLE ROW LEVEL SECURITY,
    FORCE ROW LEVEL SECURITY;

CREATE TRIGGER direct_discount_promotions_set_updated_at
    BEFORE UPDATE ON direct_discount_promotions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE FUNCTION bump_promotion_updated_at_from_direct_discount()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE promotions SET updated_at = now()
    WHERE uuid = NEW.uuid
      AND promotionable_type = 'direct';
    RETURN NEW;
END;
$$;

CREATE TRIGGER direct_discount_promotions_bump_promotion_updated_at
    AFTER UPDATE ON direct_discount_promotions
    FOR EACH ROW EXECUTE FUNCTION bump_promotion_updated_at_from_direct_discount();
