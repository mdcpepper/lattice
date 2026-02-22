UPDATE api_tokens
SET last_used_at = now()
WHERE uuid = $1
    AND revoked_at IS NULL
    AND (expires_at IS NULL OR expires_at > now())
