UPDATE products
SET deleted_at = now()
WHERE uuid = $1
