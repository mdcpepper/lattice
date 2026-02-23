//! Products service.

use async_trait::async_trait;
use jiff::Timestamp;
use mockall::automock;

use crate::{
    database::Db,
    domain::{
        products::{
            errors::ProductsServiceError,
            models::{NewProduct, Product, ProductUpdate, ProductUuid},
            repository::PgProductsRepository,
        },
        tenants::models::TenantUuid,
    },
};

#[derive(Debug, Clone)]
pub struct PgProductsService {
    db: Db,
    repository: PgProductsRepository,
}

impl PgProductsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            repository: PgProductsRepository::new(),
        }
    }
}

#[async_trait]
impl ProductsService for PgProductsService {
    async fn list_products(
        &self,
        tenant: TenantUuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<Product>, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let products = self
            .repository
            .list_products(&mut tx, point_in_time)
            .await?;

        tx.commit().await?;

        Ok(products)
    }

    async fn get_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        point_in_time: Timestamp,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let product = self
            .repository
            .get_product(&mut tx, product, point_in_time)
            .await?;

        tx.commit().await?;

        Ok(product)
    }

    async fn create_product(
        &self,
        tenant: TenantUuid,
        product: NewProduct,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let created = self
            .repository
            .create_product(&mut tx, product.uuid, product.price)
            .await?;

        tx.commit().await?;

        Ok(created)
    }

    async fn update_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        update: ProductUpdate,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let updated = self
            .repository
            .update_product(&mut tx, product, update.uuid, update.price)
            .await?;

        tx.commit().await?;

        Ok(updated)
    }

    async fn delete_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
    ) -> Result<(), ProductsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.repository.delete_product(&mut tx, product).await?;

        if rows_affected == 0 {
            return Err(ProductsServiceError::NotFound);
        }

        tx.commit().await?;

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
    ) -> Result<Vec<Product>, ProductsServiceError>;

    /// Retrieve a single product.
    async fn get_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        point_in_time: Timestamp,
    ) -> Result<Product, ProductsServiceError>;

    /// Creates a new product with the given UUID and price.
    async fn create_product(
        &self,
        tenant: TenantUuid,
        product: NewProduct,
    ) -> Result<Product, ProductsServiceError>;

    /// Updates a product with the given UUID and update.
    async fn update_product(
        &self,
        tenant: TenantUuid,
        product: ProductUuid,
        update: ProductUpdate,
    ) -> Result<Product, ProductsServiceError>;

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
    use testresult::TestResult;

    use crate::{
        domain::products::models::{NewProduct, ProductUpdate},
        test::TestContext,
    };

    use super::*;

    #[tokio::test]
    async fn create_product_returns_correct_uuid_and_price() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        let product = ctx
            .products
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 999 })
            .await?;

        assert_eq!(product.uuid, uuid);
        assert_eq!(product.price, 999);
        assert!(product.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn get_product_returns_created_product() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = ProductUuid::new();

        ctx.products
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 1500 })
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
                },
            )
            .await?;

        ctx.products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: uuid_b,
                    price: 200,
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
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 500 })
            .await?;

        let updated = ctx
            .products
            .update_product(
                ctx.tenant_uuid,
                uuid,
                ProductUpdate {
                    uuid: None,
                    price: 750,
                },
            )
            .await?;

        assert_eq!(updated.uuid, uuid);
        assert_eq!(updated.price, 750);

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
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 300 })
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
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 100 })
            .await?;

        let result = ctx
            .products
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 200 })
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
            .create_product(ctx.tenant_uuid, NewProduct { uuid, price: 100 })
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
