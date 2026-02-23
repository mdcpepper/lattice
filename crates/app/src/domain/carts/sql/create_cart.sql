INSERT INTO carts
    (uuid)
VALUES
    ($1)
RETURNING
    uuid,
    subtotal,
    total,
    0::bigint AS cart_items_count,
    created_at,
    updated_at,
    deleted_at
