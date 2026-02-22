UPDATE carts
SET deleted_at = now()
WHERE uuid = $1
  AND deleted_at IS NULL
