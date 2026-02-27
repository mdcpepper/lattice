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
VALUES
  ($1, $1, $2, $3, $4::simple_discount_kind, $5, $6)
