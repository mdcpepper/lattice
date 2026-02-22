INSERT INTO api_tokens (uuid, tenant_uuid, version, token_hash, expires_at)
VALUES ($1, $2, $3, $4, $5)
RETURNING uuid, tenant_uuid, version, created_at, last_used_at, expires_at, revoked_at
