SET LOCAL lock_timeout = '5s';

ALTER TABLE products
    ADD COLUMN tenant_uuid uuid NOT NULL;
