CREATE TABLE products (
    uuid uuid PRIMARY KEY,
    price bigint NOT NULL CHECK (price >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
