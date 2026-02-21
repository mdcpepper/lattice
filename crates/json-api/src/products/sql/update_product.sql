UPDATE products
SET
    price = $3,
    updated_at = NOW()
WHERE uuid = $1
    AND tenant_uuid = $2
    AND deleted_at IS NULL
RETURNING uuid, price, created_at, updated_at, deleted_at
