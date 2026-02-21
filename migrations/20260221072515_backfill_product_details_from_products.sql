INSERT INTO product_details
    (
        uuid,
        product_uuid,
        price,
        valid_period,
        created_at
    )
SELECT
    uuid,
    uuid,
    price,
    CASE
        WHEN deleted_at IS NULL THEN tstzrange(created_at, NULL, '[)')
        WHEN deleted_at > created_at THEN tstzrange(created_at, deleted_at, '[)')
        ELSE tstzrange(created_at, created_at + interval '1 microsecond', '[)')
    END,
    created_at
FROM products;
