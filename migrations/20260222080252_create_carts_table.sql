CREATE TABLE carts (
    uuid uuid PRIMARY KEY,
    subtotal bigint NOT NULL DEFAULT '0',
    total bigint NOT NULL DEFAULT '0',
    tenant_uuid uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);
