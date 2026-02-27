SET
  LOCAL lock_timeout = '5s';

CREATE TYPE SIMPLE_DISCOUNT_KIND AS ENUM('percentage_off', 'amount_off');

CREATE TABLE direct_discount_promotions (
  uuid UUID PRIMARY KEY,
  promotion_uuid UUID NOT NULL,

  redemption_budget BIGINT CHECK (redemption_budget >= 0),
  monetary_budget BIGINT CHECK (monetary_budget >= 0),

  discount_kind SIMPLE_DISCOUNT_KIND NOT NULL,
  discount_percentage BIGINT CHECK (discount_percentage > 0),
  discount_amount BIGINT CHECK (discount_amount > 0),

  valid_period TSTZRANGE NOT NULL DEFAULT tstzrange (now(), NULL, '[)'),

  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (NOT isempty(valid_period)),

  CHECK (
    (
      discount_kind = 'percentage_off'
      AND discount_percentage IS NOT NULL
      AND discount_amount IS NULL
    )
    OR (
      discount_kind = 'amount_off'
      AND discount_amount IS NOT NULL
      AND discount_percentage IS NULL
    )
  ),

  CONSTRAINT direct_discount_promotions_promotion_fk FOREIGN KEY (promotion_uuid) REFERENCES promotions (uuid) ON DELETE CASCADE,
  CONSTRAINT direct_discount_promotions_no_overlap_exclude EXCLUDE USING GIST (
    promotion_uuid
    WITH
      =,
      valid_period
    WITH
      &&
  ) DEFERRABLE
);

CREATE INDEX direct_discount_promotions_promotion_uuid_idx ON direct_discount_promotions (promotion_uuid);

CREATE INDEX direct_discount_promotions_created_at_idx ON direct_discount_promotions (created_at);

CREATE UNIQUE INDEX direct_discount_promotions_current_idx ON direct_discount_promotions (promotion_uuid)
WHERE
  upper_inf(valid_period);

CREATE POLICY direct_discount_promotions_tenant_select_policy ON direct_discount_promotions FOR
SELECT
  USING (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = direct_discount_promotions.promotion_uuid
        AND p.promotionable_type = 'direct'
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
    )
  );

CREATE POLICY direct_discount_promotions_tenant_insert_policy ON direct_discount_promotions FOR INSERT
WITH
  CHECK (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = direct_discount_promotions.promotion_uuid
        AND p.promotionable_type = 'direct'
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
        AND p.deleted_at IS NULL
    )
  );

CREATE POLICY direct_discount_promotions_tenant_update_policy ON direct_discount_promotions
FOR UPDATE
  USING (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = direct_discount_promotions.promotion_uuid
        AND p.promotionable_type = 'direct'
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
        AND p.deleted_at IS NULL
    )
  )
WITH
  CHECK (
    EXISTS (
      SELECT
        1
      FROM
        promotions p
      WHERE
        p.uuid = direct_discount_promotions.promotion_uuid
        AND p.promotionable_type = 'direct'
        AND p.tenant_uuid = NULLIF(
          current_setting('app.current_tenant_uuid', TRUE),
          ''
        )::uuid
        AND p.deleted_at IS NULL
    )
  );

CREATE POLICY direct_discount_promotions_tenant_delete_policy ON direct_discount_promotions FOR DELETE USING (
  EXISTS (
    SELECT
      1
    FROM
      promotions p
    WHERE
      p.uuid = direct_discount_promotions.promotion_uuid
      AND p.promotionable_type = 'direct'
      AND p.tenant_uuid = NULLIF(
        current_setting('app.current_tenant_uuid', TRUE),
        ''
      )::uuid
  )
);

ALTER TABLE direct_discount_promotions ENABLE ROW LEVEL SECURITY,
FORCE ROW LEVEL SECURITY;

CREATE FUNCTION bump_direct_discount_promotion_updated_at () RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    UPDATE promotions SET updated_at = now()
    WHERE uuid = NEW.promotion_uuid
      AND promotionable_type = 'direct';
    RETURN NEW;
END;
$$;

CREATE TRIGGER direct_discount_promotions_bump_updated_at
AFTER INSERT
OR
UPDATE ON direct_discount_promotions FOR EACH ROW
EXECUTE FUNCTION bump_direct_discount_promotion_updated_at ();
