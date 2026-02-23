UPDATE cart_items
SET deleted_at = now()
WHERE
    uuid = $1
    AND cart_uuid = $2
    AND deleted_at IS NULL
