SELECT
    uuid,
    price,
    created_at,
    updated_at,
    deleted_at
FROM products
WHERE uuid = $1 AND deleted_at IS NULL
ORDER BY created_at DESC
