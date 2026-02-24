INSERT INTO
  product_details (
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
    tstzrange ($3::TIMESTAMPTZ, NULL, '[)'),
    $3::TIMESTAMPTZ
  )
