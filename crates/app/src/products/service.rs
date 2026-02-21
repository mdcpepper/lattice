//! Products service.

use async_trait::async_trait;
use mockall::automock;
use uuid::Uuid;

use crate::{
    database::Db,
    products::{
        errors::ProductsServiceError,
        models::{NewProduct, Product, ProductUpdate},
        repository::PgProductsRepository,
    },
    tenants::models::TenantUuid,
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

    async fn begin_tenant_transaction(
        &self,
        tenant: TenantUuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, ProductsServiceError> {
        self.db
            .begin_tenant_transaction(tenant)
            .await
            .map_err(Into::into)
    }
}

#[async_trait]
impl ProductsService for PgProductsService {
    async fn list_products(
        &self,
        tenant: TenantUuid,
    ) -> Result<Vec<Product>, ProductsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let products = self.repository.list_products(&mut tx).await?;

        tx.commit().await?;

        Ok(products)
    }

    async fn get_product(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let product = self.repository.get_product(&mut tx, uuid).await?;

        tx.commit().await?;

        Ok(product)
    }

    async fn create_product(
        &self,
        tenant: TenantUuid,
        product: NewProduct,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let created = self
            .repository
            .create_product(&mut tx, product.uuid, i64::try_from(product.price)?)
            .await?;

        tx.commit().await?;

        Ok(created)
    }

    async fn update_product(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
        update: ProductUpdate,
    ) -> Result<Product, ProductsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let updated = self
            .repository
            .update_product(&mut tx, uuid, i64::try_from(update.price)?)
            .await?;

        tx.commit().await?;

        Ok(updated)
    }

    async fn delete_product(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
    ) -> Result<(), ProductsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.repository.delete_product(&mut tx, uuid).await?;

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
    async fn list_products(&self, tenant: TenantUuid)
    -> Result<Vec<Product>, ProductsServiceError>;

    /// Retrieve a single product.
    async fn get_product(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
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
        uuid: Uuid,
        update: ProductUpdate,
    ) -> Result<Product, ProductsServiceError>;

    /// Deletes a product with the given UUID.
    async fn delete_product(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
    ) -> Result<(), ProductsServiceError>;
}
