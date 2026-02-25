CREATE TYPE QUALIFICATION_CONTEXT AS ENUM('primary', 'group');

CREATE TYPE QUALIFICATION_OP AS ENUM('and', 'or');

CREATE TABLE qualifications (
  uuid UUID PRIMARY KEY,
  promotion_uuid UUID NOT NULL,

  context QUALIFICATION_CONTEXT NOT NULL DEFAULT 'primary',
  op QUALIFICATION_OP NOT NULL DEFAULT 'and',

  parent_qualification_uuid UUID,

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT qualifications_promotion_fk FOREIGN KEY (promotion_uuid) REFERENCES promotions (uuid) ON DELETE CASCADE,
  CONSTRAINT qualifications_parent_fk FOREIGN KEY (parent_qualification_uuid) REFERENCES qualifications (uuid) ON DELETE CASCADE
);

CREATE INDEX qualifications_promotion_uuid_idx ON qualifications (promotion_uuid);

CREATE INDEX qualifications_parent_qualification_uuid_idx ON qualifications (parent_qualification_uuid)
WHERE
  parent_qualification_uuid IS NOT NULL;

CREATE INDEX qualifications_created_at_idx ON qualifications (created_at);

ALTER TABLE qualifications ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;

CREATE POLICY qualifications_tenant_select_policy ON qualifications FOR
SELECT
  USING (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = qualifications.promotion_uuid
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
    )
  );

CREATE POLICY qualifications_tenant_insert_policy ON qualifications FOR INSERT
WITH
  CHECK (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = qualifications.promotion_uuid
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
        AND p.deleted_at IS NULL
    )
  );

CREATE POLICY qualifications_tenant_delete_policy ON qualifications FOR DELETE USING (
  EXISTS (
    SELECT
      1
    FROM
      promotions p
    WHERE
      p.uuid = qualifications.promotion_uuid
      AND p.tenant_uuid = NULLIF(
        current_setting('app.current_tenant_uuid', TRUE),
        ''
      )::uuid
  )
);
