WITH inserted_product AS (
    INSERT INTO products
        (uuid)
    VALUES
        ($1)
    RETURNING
        uuid,
        created_at,
        updated_at,
        deleted_at
),
inserted_product_detail AS (
    INSERT INTO product_details
        (
            uuid,
            product_uuid,
            price,
            valid_period,
            created_at
        )
    SELECT
        inserted_product.uuid,
        inserted_product.uuid,
        $2,
        tstzrange(inserted_product.created_at, NULL, '[)'),
        inserted_product.created_at
    FROM inserted_product
    RETURNING
        product_uuid,
        price
)
SELECT
    inserted_product.uuid,
    inserted_product_detail.price,
    inserted_product.created_at,
    inserted_product.updated_at,
    inserted_product.deleted_at
FROM inserted_product
INNER JOIN inserted_product_detail
    ON inserted_product_detail.product_uuid = inserted_product.uuid
