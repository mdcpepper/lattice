SET
  LOCAL lock_timeout = '5s';

CREATE TABLE api_tokens (
  uuid UUID PRIMARY KEY,

  tenant_uuid UUID NOT NULL DEFAULT NULLIF(
    current_setting('app.current_tenant_uuid', TRUE),
    ''
  )::uuid,

  version SMALLINT NOT NULL DEFAULT 1 CHECK (version > 0),
  token_hash TEXT NOT NULL,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ,
  revoked_at TIMESTAMPTZ,

  CHECK (
    expires_at IS NULL
    OR expires_at > created_at
  ),

  CONSTRAINT api_tokens_tenant_fk FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
);

CREATE INDEX api_tokens_tenant_uuid_idx ON api_tokens (tenant_uuid);

CREATE INDEX api_tokens_expires_at_idx ON api_tokens (expires_at)
WHERE
  expires_at IS NOT NULL;

CREATE INDEX api_tokens_active_tenant_idx ON api_tokens (tenant_uuid)
WHERE
  revoked_at IS NULL;
