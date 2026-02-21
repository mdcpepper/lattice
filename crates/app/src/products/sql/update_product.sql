UPDATE
    products
SET
    price = $2,
    updated_at = NOW()
WHERE
    uuid = $1
AND deleted_at IS NULL
RETURNING
    uuid,
    price,
    created_at,
    updated_at,
    deleted_at
