SET
  LOCAL lock_timeout = '5s';

CREATE TABLE tags (
  uuid UUID PRIMARY KEY,

  tenant_uuid UUID NOT NULL DEFAULT NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid,

  name TEXT NOT NULL CHECK (btrim(name) <> ''),

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at TIMESTAMPTZ,

  CONSTRAINT tags_tenant_uuid_uuid_uniq UNIQUE (tenant_uuid, uuid),
  CONSTRAINT tags_tenant_name_uniq UNIQUE (tenant_uuid, name),
  CONSTRAINT tags_tenant_fk FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
);

CREATE INDEX tags_name_idx ON tags (name);

CREATE POLICY tags_tenant_select_policy ON tags FOR
SELECT
  USING (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY tags_tenant_insert_policy ON tags FOR INSERT
WITH
  CHECK (
    tenant_uuid = NULLIF(
      current_setting('app.current_tenant_uuid', TRUE),
      ''
    )::uuid
  );

CREATE POLICY tags_tenant_delete_policy ON tags FOR DELETE USING (
  tenant_uuid = NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid
);

ALTER TABLE tags ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;
