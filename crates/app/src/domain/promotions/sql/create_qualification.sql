INSERT INTO
  qualifications (
    uuid,
    promotion_uuid,
    context,
    op,
    parent_qualification_uuid
  )
VALUES
  (
    $1,
    $2,
    $3::qualification_context,
    $4::qualification_op,
    $5
  )
