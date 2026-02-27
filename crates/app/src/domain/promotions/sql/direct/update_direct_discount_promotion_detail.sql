WITH
  target_promotion AS (
    SELECT
      uuid
    FROM
      promotions
    WHERE
      uuid = $1
      AND promotionable_type = 'direct'
      AND deleted_at IS NULL
  ),
  closed_current_version AS (
    UPDATE direct_discount_promotions
    SET
      valid_period = tstzrange (lower(valid_period), NOW(), '[)')
    WHERE
      promotion_uuid = (
        SELECT
          uuid
        FROM
          target_promotion
      )
      AND upper_inf(valid_period)
    RETURNING
      promotion_uuid
  ),
  inserted_detail AS (
    INSERT INTO
      direct_discount_promotions (
        uuid,
        promotion_uuid,
        redemption_budget,
        monetary_budget,
        discount_kind,
        discount_percentage,
        discount_amount
      )
    SELECT
      $2,
      closed_current_version.promotion_uuid,
      $3,
      $4,
      $5::simple_discount_kind,
      $6,
      $7
    FROM
      closed_current_version
    RETURNING
      uuid
  )
SELECT
  uuid
FROM
  inserted_detail
