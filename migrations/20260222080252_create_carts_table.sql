SET
  LOCAL lock_timeout = '5s';

CREATE TABLE carts (
  uuid UUID PRIMARY KEY,

  subtotal BIGINT NOT NULL DEFAULT 0,
  total    BIGINT NOT NULL DEFAULT 0,

  tenant_uuid UUID NOT NULL DEFAULT NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ,

  CONSTRAINT carts_tenant_uuid_uuid_uniq UNIQUE (tenant_uuid, uuid),
  CONSTRAINT carts_tenant_fk FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
);

CREATE POLICY carts_tenant_select_policy ON carts FOR
SELECT
  USING (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY carts_tenant_insert_policy ON carts FOR INSERT
WITH
  CHECK (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY carts_tenant_update_policy ON carts
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

CREATE POLICY carts_tenant_delete_policy ON carts FOR DELETE USING (
  tenant_uuid = NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid
);

ALTER TABLE carts ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;
