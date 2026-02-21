SELECT uuid, tenant_uuid, token_hash
FROM api_tokens
WHERE token_hash = $1
    AND revoked_at IS NULL
LIMIT 1
