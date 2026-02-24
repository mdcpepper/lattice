SELECT
  products.uuid,
  product_details.price,
  products.created_at,
  products.updated_at,
  products.deleted_at
FROM
  products
  INNER JOIN product_details ON product_details.product_uuid = products.uuid
WHERE
  product_details.valid_period @> $1::TIMESTAMPTZ
  AND products.created_at <= $1::TIMESTAMPTZ
  AND (
    products.deleted_at IS NULL
    OR products.deleted_at > $1::TIMESTAMPTZ
  )
ORDER BY
  products.created_at DESC
