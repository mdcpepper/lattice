WITH inserted_tenant AS (
    INSERT INTO tenants (uuid, name)
    VALUES ($1, $2)
    RETURNING uuid, name, created_at, created_at AS updated_at, deleted_at
),
inserted_token AS (
    INSERT INTO api_tokens (uuid, tenant_uuid, token_hash)
    VALUES ($3, $1, $4)
)
SELECT uuid, name, created_at, updated_at, deleted_at
FROM inserted_tenant;
