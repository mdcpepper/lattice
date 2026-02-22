WITH inserted_tenant AS (
    INSERT INTO tenants (uuid, name)
    VALUES ($1, $2)
    RETURNING uuid, name, created_at, created_at AS updated_at, deleted_at
)
SELECT uuid, name, created_at, updated_at, deleted_at
FROM inserted_tenant;
