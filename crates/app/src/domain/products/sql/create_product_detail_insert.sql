INSERT INTO product_details
    (
        uuid,
        product_uuid,
        price,
        valid_period,
        created_at
    )
VALUES
    (
        $1,
        $1,
        $2,
        tstzrange($3::timestamptz, NULL, '[)'),
        $3::timestamptz
    )
