CREATE TABLE api_tokens (
    uuid uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants(uuid),
    version smallint NOT NULL DEFAULT 1 CHECK (version > 0),
    token_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    expires_at timestamptz,
    revoked_at timestamptz,
    CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE INDEX api_tokens_tenant_uuid_idx ON api_tokens (tenant_uuid);
CREATE INDEX api_tokens_expires_at_idx ON api_tokens (expires_at)
    WHERE expires_at IS NOT NULL;
CREATE INDEX api_tokens_active_tenant_idx ON api_tokens (tenant_uuid)
    WHERE revoked_at IS NULL;
