WITH
  new_tags AS (
    SELECT
      unnest($1::uuid[]) AS uuid,
      unnest($2::TEXT[]) AS name
  ),
  inserted AS (
    INSERT INTO
      tags (uuid, name)
    SELECT
      uuid,
      name
    FROM
      new_tags
    ON CONFLICT ON CONSTRAINT tags_tenant_name_uniq DO NOTHING
    RETURNING
      uuid,
      name
  )
SELECT
  COALESCE(i.uuid, t.uuid) AS uuid,
  n.name
FROM
  new_tags n
  LEFT JOIN inserted i ON i.name = n.name
  LEFT JOIN tags t ON t.name = n.name
