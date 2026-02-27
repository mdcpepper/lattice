INSERT INTO
  qualifications (
    uuid,
    promotion_uuid,
    promotionable_uuid,
    context,
    op,
    parent_qualification_uuid,
    promotionable_type
  )
VALUES
  (
    $1,
    $2,
    $3,
    $4::qualification_context,
    $5::qualification_op,
    $6,
    $7::promotionable_type
  )
