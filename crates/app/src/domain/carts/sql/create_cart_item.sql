INSERT INTO cart_items
    (
        uuid,
        base_price,
        cart_uuid,
        product_uuid
    )
SELECT
    $1,
    pd.price,
    $2,
    $3
FROM product_details pd
WHERE pd.product_uuid = $3
  AND pd.valid_period @> now()
RETURNING
    uuid,
    base_price,
    cart_uuid,
    product_uuid,
    created_at,
    updated_at,
    deleted_at
