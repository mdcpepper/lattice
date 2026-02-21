SELECT uuid, price, created_at, updated_at, deleted_at
FROM products
WHERE deleted_at IS NULL
    AND tenant_uuid = $1
ORDER BY created_at DESC
