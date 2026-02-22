INSERT INTO carts
    (uuid)
VALUES
    ($1)
RETURNING
    uuid,
    subtotal,
    total,
    created_at,
    updated_at,
    deleted_at
