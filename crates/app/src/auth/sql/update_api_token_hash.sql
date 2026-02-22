UPDATE api_tokens
SET token_hash = $2
WHERE uuid = $1
    AND revoked_at IS NULL
    AND (expires_at IS NULL OR expires_at > now())
