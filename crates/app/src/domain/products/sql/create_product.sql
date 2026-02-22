INSERT INTO products
    (uuid)
VALUES
    ($1)
RETURNING
    uuid,
    created_at,
    updated_at,
    deleted_at
