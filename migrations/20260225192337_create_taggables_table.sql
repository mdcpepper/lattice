SET
  LOCAL lock_timeout = '5s';

CREATE TYPE TAGGABLE_TYPE AS ENUM('product', 'qualification_rule');

CREATE TABLE taggables (
  tag_uuid UUID NOT NULL,

  taggable_type TAGGABLE_TYPE NOT NULL,
  taggable_uuid UUID NOT NULL,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT taggables_pk PRIMARY KEY (tag_uuid, taggable_type, taggable_uuid),
  CONSTRAINT taggables_tag_fk FOREIGN KEY (tag_uuid) REFERENCES tags (uuid) ON DELETE CASCADE
);

CREATE INDEX taggables_taggable_lookup_idx ON taggables (taggable_type, taggable_uuid);

CREATE POLICY taggables_tenant_select_policy ON taggables FOR
SELECT
  USING (
    EXISTS (
      SELECT
        1
      FROM
        tags t
      WHERE
        t.uuid = taggables.tag_uuid
        AND t.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
    )
    AND (
      (
        taggables.taggable_type = 'product'
        AND EXISTS (
          SELECT
            1
          FROM
            products p
          WHERE
            p.uuid = taggables.taggable_uuid
            AND p.tenant_uuid = NULLIF(
              current_setting('app.current_tenant_uuid', TRUE),
              ''
            )::uuid
        )
      )
      OR (
        taggables.taggable_type = 'qualification_rule'
        AND EXISTS (
          SELECT
            1
          FROM
            qualification_rules qr
            JOIN qualifications q ON q.uuid = qr.qualification_uuid
            JOIN promotions p ON p.uuid = q.promotion_uuid
          WHERE
            qr.uuid = taggables.taggable_uuid
            AND p.promotionable_type = q.promotionable_type
            AND p.tenant_uuid = NULLIF(
              current_setting('app.current_tenant_uuid', TRUE),
              ''
            )::uuid
        )
      )
    )
  );

CREATE POLICY taggables_tenant_insert_policy ON taggables FOR INSERT
WITH
  CHECK (
    EXISTS (
      SELECT
        1
      FROM
        tags t
      WHERE
        t.uuid = taggables.tag_uuid
        AND t.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
    )
    AND (
      (
        taggables.taggable_type = 'product'
        AND EXISTS (
          SELECT
            1
          FROM
            products p
          WHERE
            p.uuid = taggables.taggable_uuid
            AND p.tenant_uuid = NULLIF(
              current_setting('app.current_tenant_uuid', TRUE),
              ''
            )::uuid
            AND p.deleted_at IS NULL
        )
      )
      OR (
        taggables.taggable_type = 'qualification_rule'
        AND EXISTS (
          SELECT
            1
          FROM
            qualification_rules qr
            JOIN qualifications q ON q.uuid = qr.qualification_uuid
            JOIN promotions p ON p.uuid = q.promotion_uuid
          WHERE
            qr.uuid = taggables.taggable_uuid
            AND p.promotionable_type = q.promotionable_type
            AND p.tenant_uuid = NULLIF(
              current_setting('app.current_tenant_uuid', TRUE),
              ''
            )::uuid
            AND p.deleted_at IS NULL
        )
      )
    )
  );

CREATE POLICY taggables_tenant_delete_policy ON taggables FOR DELETE USING (
  EXISTS (
    SELECT
      1
    FROM
      tags t
    WHERE
      t.uuid = taggables.tag_uuid
      AND t.tenant_uuid = NULLIF(
        current_setting('app.current_tenant_uuid', TRUE),
        ''
      )::uuid
  )
  AND (
    (
      taggables.taggable_type = 'product'
      AND EXISTS (
        SELECT
          1
        FROM
          products p
        WHERE
          p.uuid = taggables.taggable_uuid
          AND p.tenant_uuid = NULLIF(
            current_setting('app.current_tenant_uuid', TRUE),
            ''
          )::uuid
      )
    )
    OR (
      taggables.taggable_type = 'qualification_rule'
      AND EXISTS (
        SELECT
          1
        FROM
          qualification_rules qr
          JOIN qualifications q ON q.uuid = qr.qualification_uuid
          JOIN promotions p ON p.uuid = q.promotion_uuid
        WHERE
          qr.uuid = taggables.taggable_uuid
          AND p.promotionable_type = q.promotionable_type
          AND p.tenant_uuid = NULLIF(
            current_setting('app.current_tenant_uuid', TRUE),
            ''
          )::uuid
      )
    )
  )
);

ALTER TABLE taggables ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;
