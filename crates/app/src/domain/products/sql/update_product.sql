WITH
  target_product AS (
    SELECT
      uuid
    FROM
      products
    WHERE
      uuid = $1
      AND deleted_at IS NULL
  ),
  closed_current_detail AS (
    UPDATE product_details
    SET
      valid_period = tstzrange (lower(valid_period), NOW(), '[)')
    WHERE
      product_uuid = (
        SELECT
          uuid
        FROM
          target_product
      )
      AND upper_inf(valid_period)
    RETURNING
      product_uuid
  ),
  inserted_detail AS (
    INSERT INTO
      product_details (
        uuid,
        product_uuid,
        price,
        valid_period,
        created_at
      )
    SELECT
      $2,
      closed_current_detail.product_uuid,
      $3,
      tstzrange (NOW(), NULL, '[)'),
      NOW()
    FROM
      closed_current_detail
    RETURNING
      product_uuid,
      price
  ),
  updated_product AS (
    UPDATE products
    SET
      updated_at = NOW()
    WHERE
      uuid = (
        SELECT
          product_uuid
        FROM
          inserted_detail
      )
    RETURNING
      uuid,
      created_at,
      updated_at,
      deleted_at
  )
SELECT
  updated_product.uuid,
  inserted_detail.price,
  updated_product.created_at,
  updated_product.updated_at,
  updated_product.deleted_at
FROM
  updated_product
  INNER JOIN inserted_detail ON inserted_detail.product_uuid = updated_product.uuid
