INSERT INTO products
    (uuid, price)
VALUES
    ($1, $2)
RETURNING
    uuid,
    price,
    created_at,
    updated_at,
    deleted_at
