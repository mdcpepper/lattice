INSERT INTO
  promotions (uuid, promotionable_type)
VALUES
  ($1, $2::promotionable_type)
RETURNING
  uuid,
  created_at,
  updated_at,
  deleted_at
