SET LOCAL lock_timeout = '5s';

CREATE TABLE product_details (
    uuid         uuid PRIMARY KEY,
    product_uuid uuid NOT NULL,
    price        bigint NOT NULL CHECK (price >= 0),
    valid_period tstzrange NOT NULL DEFAULT tstzrange(now(), NULL, '[)'),
    created_at   timestamptz NOT NULL DEFAULT now(),

    CHECK (NOT isempty(valid_period)),

    CONSTRAINT product_details_product_fk
        FOREIGN KEY (product_uuid)
        REFERENCES products (uuid)
        ON DELETE CASCADE,

    CONSTRAINT product_details_no_overlap_exclude
        EXCLUDE USING GIST (product_uuid WITH =, valid_period WITH &&) DEFERRABLE
);

CREATE INDEX product_details_product_uuid_idx ON product_details (product_uuid);
CREATE INDEX product_details_created_at_idx ON product_details (created_at);
CREATE UNIQUE INDEX product_details_current_idx ON product_details (product_uuid) WHERE upper_inf(valid_period);

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
