INSERT INTO
  taggables (tag_uuid, taggable_type, taggable_uuid)
SELECT
  x.tag_uuid,
  $3::taggable_type,
  x.taggable_uuid
FROM
  unnest($1::uuid[], $2::uuid[]) AS x (tag_uuid, taggable_uuid)
ON CONFLICT DO NOTHING
