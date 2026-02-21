SET LOCAL lock_timeout = '5s';

ALTER TABLE products
    VALIDATE CONSTRAINT products_tenant_uuid_fkey;
