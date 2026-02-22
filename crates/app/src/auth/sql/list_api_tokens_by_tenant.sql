SELECT uuid, tenant_uuid, version, created_at, last_used_at, expires_at, revoked_at
FROM api_tokens
WHERE tenant_uuid = $1
ORDER BY created_at DESC
