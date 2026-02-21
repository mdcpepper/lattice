CREATE TABLE product_details (
    uuid uuid PRIMARY KEY,
    product_uuid uuid NOT NULL REFERENCES products(uuid) ON DELETE CASCADE,
    price bigint NOT NULL CHECK (price >= 0),
    valid_period tstzrange NOT NULL DEFAULT tstzrange(now(), NULL, '[)'),
    created_at timestamptz NOT NULL DEFAULT now(),

    CHECK (NOT isempty(valid_period))
);

CREATE INDEX product_details_product_uuid_idx ON product_details(product_uuid);
CREATE INDEX product_details_created_at_idx ON product_details(created_at);

ALTER TABLE product_details
    ADD CONSTRAINT product_details_no_overlap_exclude
    EXCLUDE USING GIST (product_uuid WITH =, valid_period WITH &&) DEFERRABLE;

CREATE UNIQUE INDEX product_details_current_idx
    ON product_details(product_uuid)
    WHERE upper_inf(valid_period);
