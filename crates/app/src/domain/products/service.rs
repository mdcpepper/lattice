//! Products Service.

use async_trait::async_trait;
use jiff::Timestamp;
use mockall::automock;
use tracing::{info, warn};

use crate::{
    database::Db,
    domain::{
        products::{
            data::{NewProduct, ProductUpdate},
            errors::ProductsServiceError,
            records::{ProductRecord, ProductUuid},
            repository::PgProductsRepository,
        },
        tags::PgTagsRepository,
        tenants::records::TenantUuid,
    },
};

#[derive(Debug, Clone)]
pub struct PgProductsService {
    db: Db,
    products: PgProductsRepository,
    tags: PgTagsRepository,
}

impl PgProductsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            products: PgProductsRepository::new(),
            tags: PgTagsRepository::new(),
        }
    }

    #[cfg(test)]
    #[tracing::instrument(
        name = "products.service.list_product_tags",
        skip(self),
        fields(tenant_uuid = %tenant, product_uuid = %product),
        err
    )]
    async fn list_product_tags(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
    ) -> Result<Vec<String>, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let names = self.tags.list_taggable_tag_names(&mut tx, product).await?;

        tx.commit().await?;

        Ok(names)
    }
}

#[async_trait]
impl ProductsService for PgProductsService {
    #[tracing::instrument(
        name = "products.service.list_products",
        skip(self),
        fields(tenant_uuid = %tenant, point_in_time = %point_in_time),
        err
    )]
    async fn list_products(
        &self,
        tenant: TenantUuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<ProductRecord>, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let products = self.products.list_products(&mut tx, point_in_time).await?;
        let product_count = products.len();

        tx.commit().await?;

        info!(product_count, "listed products");

        Ok(products)
    }

    #[tracing::instrument(
        name = "products.service.get_product",
        skip(self),
        fields(
            tenant_uuid = %tenant,
            product_uuid = %product,
            point_in_time = %point_in_time
        ),
        err
    )]
    async fn get_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        point_in_time: Timestamp,
    ) -> Result<ProductRecord, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let product = self
            .products
            .get_product(&mut tx, product, point_in_time)
            .await?;

        tx.commit().await?;

        info!(product_uuid = %product.uuid, "fetched product");

        Ok(product)
    }

    #[tracing::instrument(
        name = "products.service.create_product",
        skip(self),
        fields(
            tenant_uuid = %tenant,
            product_uuid = %product.uuid,
            price = product.price,
            tags_count = product.tags.len()
        ),
        err
    )]
    async fn create_product(
        &self,
        tenant: TenantUuid,
        product: NewProduct,
    ) -> Result<ProductRecord, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let NewProduct { uuid, price, tags } = product;

        let created = self.products.create_product(&mut tx, uuid, price).await?;

        let taggables = self
            .tags
            .resolve_taggable_tags(&mut tx, &[(uuid, tags)])
            .await?;

        self.tags.create_taggables(&mut tx, &taggables).await?;

        tx.commit().await?;

        info!(product_uuid = %created.uuid, price = created.price, "created product");

        Ok(created)
    }

    #[tracing::instrument(
        name = "products.service.update_product",
        skip(self),
        fields(
            tenant_uuid = %tenant,
            product_uuid = %product,
            product_details_uuid = ?update.uuid,
            price = update.price,
            tags_count = update.tags.len()
        ),
        err
    )]
    async fn update_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        update: ProductUpdate,
    ) -> Result<ProductRecord, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let ProductUpdate { uuid, price, tags } = update;

        let updated = self
            .products
            .update_product(&mut tx, product, uuid, price)
            .await?;

        self.tags.delete_taggables(&mut tx, &[product]).await?;

        let taggables = self
            .tags
            .resolve_taggable_tags(&mut tx, &[(product, tags)])
            .await?;

        self.tags.create_taggables(&mut tx, &taggables).await?;

        tx.commit().await?;

        info!(product_uuid = %updated.uuid, price = updated.price, "updated product");

        Ok(updated)
    }

    #[tracing::instrument(
        name = "products.service.delete_product",
        skip(self),
        fields(tenant_uuid = %tenant, product_uuid = %product),
        err
    )]
    async fn delete_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
    ) -> Result<(), ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.products.delete_product(&mut tx, product).await?;

        if rows_affected == 0 {
            warn!("product did not exist for deletion");

            return Err(ProductsServiceError::NotFound);
        }

        tx.commit().await?;

        info!(rows_affected, "deleted product");

        Ok(())
    }
}

#[automock]
#[async_trait]
pub trait ProductsService: Send + Sync {
    /// Retrieves all products.
    async fn list_products(
        &self,
        tenant: TenantUuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<ProductRecord>, ProductsServiceError>;

    /// Retrieve a single product.
    async fn get_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        point_in_time: Timestamp,
    ) -> Result<ProductRecord, ProductsServiceError>;

    /// Creates a new product with the given UUID and price.
    async fn create_product(
        &self,
        tenant: TenantUuid,
        product: NewProduct,
    ) -> Result<ProductRecord, ProductsServiceError>;

    /// Updates a product with the given UUID and update.
    async fn update_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        update: ProductUpdate,
    ) -> Result<ProductRecord, ProductsServiceError>;

    /// Deletes a product with the given UUID.
    async fn delete_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
    ) -> Result<(), ProductsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use smallvec::smallvec;
    use testresult::TestResult;

    use crate::{
        domain::products::data::{NewProduct, ProductUpdate},
        test::TestContext,
    };

    use super::*;

    #[tokio::test]
    async fn create_product_returns_correct_uuid_and_price() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        let product = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 999,
                    tags: smallvec![],
                },
            )
            .await?;

        assert_eq!(product.uuid, uuid);
        assert_eq!(product.price, 999);
        assert!(product.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn create_product_syncs_tags() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 999,
                    tags: smallvec!["apparel".to_string(), "sale".to_string()],
                },
            )
            .await?;

        let names = ctx
            .products
            .list_product_tags(ctx.tenant_uuid, uuid)
            .await?;

        assert_eq!(names, vec!["apparel".to_string(), "sale".to_string()]);

        Ok(())
    }

    #[tokio::test]
    async fn get_product_returns_created_product() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 1500,
                    tags: smallvec![],
                },
            )
            .await?;

        let product = ctx
            .products
            .get_product(ctx.tenant_uuid, uuid, Timestamp::now())
            .await?;

        assert_eq!(product.uuid, uuid);
        assert_eq!(product.price, 1500);
        assert!(product.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn get_product_unknown_uuid_returns_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx
            .products
            .get_product(ctx.tenant_uuid, ProductUuid::new(), Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn list_products_returns_created_products() -> TestResult {
        let ctx = TestContext::new().await;

        let uuid_a = ProductUuid::new();
        let uuid_b = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: uuid_a,
                    price: 100,
                    tags: smallvec![],
                },
            )
            .await?;

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: uuid_b,
                    price: 200,
                    tags: smallvec![],
                },
            )
            .await?;

        let products = ctx
            .products
            .list_products(ctx.tenant_uuid, Timestamp::now())
            .await?;

        let uuids: Vec<ProductUuid> = products.iter().map(|p| p.uuid).collect();

        assert!(uuids.contains(&uuid_a), "product A should be in the list");
        assert!(uuids.contains(&uuid_b), "product B should be in the list");

        Ok(())
    }

    #[tokio::test]
    async fn list_products_empty_when_none_created() -> TestResult {
        let ctx = TestContext::new().await;

        let products = ctx
            .products
            .list_products(ctx.tenant_uuid, Timestamp::now())
            .await?;

        assert!(products.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn update_product_reflects_new_price() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 500,
                    tags: smallvec![],
                },
            )
            .await?;

        let updated = ctx
            .products
            .update_product(
                ctx.tenant_uuid,
                uuid,
                ProductUpdate {
                    uuid: None,
                    price: 750,
                    tags: smallvec![],
                },
            )
            .await?;

        assert_eq!(updated.uuid, uuid);
        assert_eq!(updated.price, 750);

        Ok(())
    }

    #[tokio::test]
    async fn update_product_replaces_tags() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 500,
                    tags: smallvec!["apparel".to_string(), "sale".to_string()],
                },
            )
            .await?;

        ctx.products
            .update_product(
                ctx.tenant_uuid,
                uuid,
                ProductUpdate {
                    uuid: None,
                    price: 750,
                    tags: smallvec!["featured".to_string(), "sale".to_string()],
                },
            )
            .await?;

        let names = ctx
            .products
            .list_product_tags(ctx.tenant_uuid, uuid)
            .await?;

        assert_eq!(names, vec!["featured".to_string(), "sale".to_string()]);

        Ok(())
    }

    #[tokio::test]
    async fn update_product_can_clear_tags() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 500,
                    tags: smallvec!["apparel".to_string(), "sale".to_string()],
                },
            )
            .await?;

        ctx.products
            .update_product(
                ctx.tenant_uuid,
                uuid,
                ProductUpdate {
                    uuid: None,
                    price: 750,
                    tags: smallvec![],
                },
            )
            .await?;

        let names = ctx
            .products
            .list_product_tags(ctx.tenant_uuid, uuid)
            .await?;

        assert!(names.is_empty(), "expected product tags to be cleared");

        Ok(())
    }

    #[tokio::test]
    async fn update_product_unknown_uuid_returns_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx
            .products
            .update_product(
                ctx.tenant_uuid,
                ProductUuid::new(),
                ProductUpdate {
                    uuid: None,
                    price: 100,
                    tags: smallvec![],
                },
            )
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn delete_product_makes_it_not_found() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 300,
                    tags: smallvec![],
                },
            )
            .await?;

        ctx.products.delete_product(ctx.tenant_uuid, uuid).await?;

        let result = ctx
            .products
            .get_product(ctx.tenant_uuid, uuid, Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::NotFound)),
            "expected NotFound after deletion, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn delete_product_unknown_uuid_returns_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx
            .products
            .delete_product(ctx.tenant_uuid, ProductUuid::new())
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn create_product_duplicate_uuid_returns_already_exists() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 100,
                    tags: smallvec![],
                },
            )
            .await?;

        let result = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 200,
                    tags: smallvec![],
                },
            )
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::AlreadyExists)),
            "expected AlreadyExists, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn product_not_visible_to_other_tenant() -> TestResult {
        let ctx = TestContext::new().await;

        let product = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: ProductUuid::new(),
                    price: 100,
                    tags: smallvec![],
                },
            )
            .await?;

        let tenant_b = ctx.create_tenant("Tenant B").await;

        // Tenant B cannot see Tenant A's product
        let result = ctx
            .products
            .get_product(tenant_b, product.uuid, Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(ProductsServiceError::NotFound)),
            "expected NotFound for cross-tenant access, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn deleted_product_not_returned_in_list() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid,
                    price: 100,
                    tags: smallvec![],
                },
            )
            .await?;

        ctx.products.delete_product(ctx.tenant_uuid, uuid).await?;

        let products = ctx
            .products
            .list_products(ctx.tenant_uuid, Timestamp::now())
            .await?;

        assert!(
            !products.iter().any(|p| p.uuid == uuid),
            "deleted product should not appear in list"
        );

        Ok(())
    }
}
