SELECT
    carts.uuid,
    carts.created_at,
    carts.updated_at,
    carts.deleted_at
FROM carts
WHERE carts.uuid = $1
  AND carts.created_at <= $2::timestamptz
  AND (carts.deleted_at IS NULL OR carts.deleted_at > $2::timestamptz)
