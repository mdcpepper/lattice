SELECT
  t.name
FROM
  taggables tg
  JOIN tags t ON t.uuid = tg.tag_uuid
WHERE
  tg.taggable_type = $1::taggable_type
  AND tg.taggable_uuid = $2::uuid
ORDER BY
  t.name
