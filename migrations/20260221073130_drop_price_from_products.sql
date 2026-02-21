SET LOCAL lock_timeout = '5s';

ALTER TABLE products
    DROP COLUMN price;
