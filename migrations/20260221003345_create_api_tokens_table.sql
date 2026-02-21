CREATE TABLE api_tokens (
    uuid uuid PRIMARY KEY,
    tenant_uuid uuid NOT NULL REFERENCES tenants(uuid),
    token_hash text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    revoked_at timestamptz
);

CREATE INDEX api_tokens_token_hash_idx ON api_tokens (token_hash)
    WHERE revoked_at IS NULL;
