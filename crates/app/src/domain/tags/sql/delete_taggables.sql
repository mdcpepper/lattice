DELETE FROM taggables
WHERE
  taggable_type = $1::taggable_type
  AND taggable_uuid = ANY($2::uuid[])
