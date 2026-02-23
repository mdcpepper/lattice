SELECT
    carts.uuid,
    carts.subtotal,
    carts.total,
    (
        SELECT COUNT(*)
        FROM cart_items
        WHERE cart_items.cart_uuid = carts.uuid
          AND cart_items.created_at <= $2::timestamptz
          AND (cart_items.deleted_at IS NULL OR cart_items.deleted_at > $2::timestamptz)
    ) AS cart_items_count,
    carts.created_at,
    carts.updated_at,
    carts.deleted_at
FROM carts
WHERE carts.uuid = $1
  AND carts.created_at <= $2::timestamptz
  AND (carts.deleted_at IS NULL OR carts.deleted_at > $2::timestamptz)
