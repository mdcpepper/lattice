//! Carts service.

use async_trait::async_trait;
use jiff::Timestamp;
use mockall::automock;
use uuid::Uuid;

use crate::{
    database::Db,
    domain::{
        carts::{
            errors::CartsServiceError,
            models::{Cart, NewCart},
            repository::PgCartsRepository,
        },
        tenants::models::TenantUuid,
    },
};

#[derive(Debug, Clone)]
pub struct PgCartsService {
    db: Db,
    repository: PgCartsRepository,
}

impl PgCartsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            repository: PgCartsRepository::new(),
        }
    }

    async fn begin_tenant_transaction(
        &self,
        tenant: TenantUuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, CartsServiceError> {
        self.db
            .begin_tenant_transaction(tenant)
            .await
            .map_err(Into::into)
    }
}

#[async_trait]
impl CartsService for PgCartsService {
    async fn get_cart(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, CartsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let cart = self
            .repository
            .get_cart(&mut tx, uuid, point_in_time)
            .await?;

        tx.commit().await?;

        Ok(cart)
    }

    async fn create_cart(
        &self,
        tenant: TenantUuid,
        cart: NewCart,
    ) -> Result<Cart, CartsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let created = self.repository.create_cart(&mut tx, cart.uuid).await?;

        tx.commit().await?;

        Ok(created)
    }

    async fn delete_cart(&self, tenant: TenantUuid, uuid: Uuid) -> Result<(), CartsServiceError> {
        let mut tx = self.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.repository.delete_cart(&mut tx, uuid).await?;

        if rows_affected == 0 {
            return Err(CartsServiceError::NotFound);
        }

        tx.commit().await?;

        Ok(())
    }
}

#[automock]
#[async_trait]
pub trait CartsService: Send + Sync {
    /// Retrieve a single cart.
    async fn get_cart(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, CartsServiceError>;

    /// Creates a new cart with the given details.
    async fn create_cart(
        &self,
        tenant: TenantUuid,
        cart: NewCart,
    ) -> Result<Cart, CartsServiceError>;

    /// Deletes a cart with the given UUID.
    async fn delete_cart(&self, tenant: TenantUuid, uuid: Uuid) -> Result<(), CartsServiceError>;
}
