SELECT tenant_uuid, version, token_hash
FROM api_tokens
WHERE uuid = $1
    AND version = $2
    AND revoked_at IS NULL
    AND (expires_at IS NULL OR expires_at > now())
LIMIT 1
