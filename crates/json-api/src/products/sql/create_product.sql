INSERT INTO products (uuid, price, tenant_uuid)
VALUES ($1, $2, $3)
RETURNING uuid, price, created_at, updated_at, deleted_at
