SELECT
    products.uuid,
    product_details.price,
    products.created_at,
    products.updated_at,
    products.deleted_at
FROM products
INNER JOIN product_details
    ON product_details.product_uuid = products.uuid
WHERE products.uuid = $1
  AND product_details.valid_period @> $2::timestamptz
  AND products.created_at <= $2::timestamptz
  AND (products.deleted_at IS NULL OR products.deleted_at > $2::timestamptz)
