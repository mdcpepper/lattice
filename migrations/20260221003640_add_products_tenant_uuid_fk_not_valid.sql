SET LOCAL lock_timeout = '5s';

ALTER TABLE products
    ADD CONSTRAINT products_tenant_uuid_fkey
    FOREIGN KEY (tenant_uuid) REFERENCES tenants(uuid) NOT VALID;
