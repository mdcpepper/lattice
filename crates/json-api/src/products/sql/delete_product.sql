DELETE FROM products
WHERE uuid = $1
    AND tenant_uuid = $2
