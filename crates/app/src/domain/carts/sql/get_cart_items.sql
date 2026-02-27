SELECT
    cart_items.uuid,
    cart_items.price,
    cart_items.product_uuid,
    cart_items.created_at,
    cart_items.updated_at,
    cart_items.deleted_at
FROM cart_items
WHERE cart_items.cart_uuid = $1
  AND cart_items.created_at <= $2::timestamptz
  AND (cart_items.deleted_at IS NULL OR cart_items.deleted_at > $2::timestamptz)
ORDER BY cart_items.created_at
