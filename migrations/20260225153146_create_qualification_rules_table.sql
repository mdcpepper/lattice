SET LOCAL lock_timeout = '5s';

CREATE TYPE QUALIFICATION_RULE_KIND AS ENUM('has_all', 'has_any', 'has_none');

CREATE TABLE qualification_rules (
  uuid UUID PRIMARY KEY,
  qualification_uuid UUID NOT NULL,
  kind QUALIFICATION_RULE_KIND NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT qualification_rules_qualification_fk FOREIGN KEY (qualification_uuid) REFERENCES qualifications (uuid) ON DELETE CASCADE
);

CREATE INDEX qualification_rules_qualification_uuid_idx ON qualification_rules (qualification_uuid);

ALTER TABLE qualification_rules ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;

CREATE POLICY qualification_rules_tenant_select_policy ON qualification_rules FOR
SELECT
  USING (
    EXISTS (
      SELECT
        1
      FROM
        qualifications q
        JOIN promotions p ON p.uuid = q.promotion_uuid
      WHERE
        q.uuid = qualification_rules.qualification_uuid
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
    )
  );

CREATE POLICY qualification_rules_tenant_insert_policy ON qualification_rules FOR INSERT
WITH
  CHECK (
    EXISTS (
      SELECT
        1
      FROM
        qualifications q
        JOIN promotions p ON p.uuid = q.promotion_uuid
      WHERE
        q.uuid = qualification_rules.qualification_uuid
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
        AND p.deleted_at IS NULL
    )
  );

CREATE POLICY qualification_rules_tenant_delete_policy ON qualification_rules FOR DELETE USING (
  EXISTS (
    SELECT
      1
    FROM
      qualifications q
      JOIN promotions p ON p.uuid = q.promotion_uuid
    WHERE
      q.uuid = qualification_rules.qualification_uuid
      AND p.tenant_uuid = NULLIF(
        current_setting('app.current_tenant_uuid', TRUE),
        ''
      )::uuid
  )
);
