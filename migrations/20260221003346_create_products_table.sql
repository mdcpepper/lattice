SET
  LOCAL lock_timeout = '5s';

CREATE TABLE products (
  uuid UUID PRIMARY KEY,

  tenant_uuid UUID NOT NULL DEFAULT NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ,

  CONSTRAINT products_tenant_uuid_uuid_uniq UNIQUE (tenant_uuid, uuid),
  CONSTRAINT products_tenant_fk FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
);

CREATE POLICY products_tenant_select_policy ON products FOR
SELECT
  USING (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY products_tenant_insert_policy ON products FOR INSERT
WITH
  CHECK (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY products_tenant_update_policy ON products
FOR UPDATE
  USING (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  )
WITH
  CHECK (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY products_tenant_delete_policy ON products FOR DELETE USING (
  tenant_uuid = NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid
);

ALTER TABLE products ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;

CREATE FUNCTION set_updated_at () RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER products_set_updated_at BEFORE
UPDATE ON products FOR EACH ROW
EXECUTE FUNCTION set_updated_at ();
