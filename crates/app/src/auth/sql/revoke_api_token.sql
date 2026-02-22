UPDATE api_tokens
SET revoked_at = now()
WHERE uuid = $1
    AND revoked_at IS NULL
RETURNING uuid, tenant_uuid, version, created_at, last_used_at, expires_at, revoked_at
