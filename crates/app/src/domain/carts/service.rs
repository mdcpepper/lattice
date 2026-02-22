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
}

#[async_trait]
impl CartsService for PgCartsService {
    async fn get_cart(
        &self,
        tenant: TenantUuid,
        uuid: Uuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

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
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let created = self.repository.create_cart(&mut tx, cart.uuid).await?;

        tx.commit().await?;

        Ok(created)
    }

    async fn delete_cart(&self, tenant: TenantUuid, uuid: Uuid) -> Result<(), CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

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

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use uuid::Uuid;

    use crate::{domain::carts::models::NewCart, test::TestContext};

    use super::*;

    #[tokio::test]
    async fn create_cart_returns_correct_uuid() {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        let cart = ctx
            .carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await
            .expect("create_cart should succeed");

        assert_eq!(cart.uuid, uuid);
        assert_eq!(cart.subtotal, 0);
        assert_eq!(cart.total, 0);
        assert!(cart.deleted_at.is_none());
    }

    #[tokio::test]
    async fn get_cart_returns_created_cart() {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await
            .expect("create_cart should succeed");

        let cart = ctx
            .carts
            .get_cart(ctx.tenant_uuid, uuid, Timestamp::now())
            .await
            .expect("get_cart should succeed");

        assert_eq!(cart.uuid, uuid);
        assert!(cart.deleted_at.is_none());
    }

    #[tokio::test]
    async fn get_cart_unknown_uuid_returns_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx
            .carts
            .get_cart(ctx.tenant_uuid, Uuid::now_v7(), Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn create_cart_duplicate_uuid_returns_already_exists() {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await
            .expect("first create_cart should succeed");

        let result = ctx
            .carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::AlreadyExists)),
            "expected AlreadyExists, got {result:?}"
        );
    }

    #[tokio::test]
    async fn delete_cart_makes_it_not_found() {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await
            .expect("create_cart should succeed");

        ctx.carts
            .delete_cart(ctx.tenant_uuid, uuid)
            .await
            .expect("delete_cart should succeed");

        let result = ctx
            .carts
            .get_cart(ctx.tenant_uuid, uuid, Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound after deletion, got {result:?}"
        );
    }

    #[tokio::test]
    async fn delete_cart_unknown_uuid_returns_not_found() {
        let ctx = TestContext::new().await;

        let result = ctx.carts.delete_cart(ctx.tenant_uuid, Uuid::now_v7()).await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn cart_not_visible_to_other_tenant() {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        let tenant_b = ctx.create_tenant("Tenant B").await;

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await
            .expect("create_cart should succeed");

        let result = ctx.carts.get_cart(tenant_b, uuid, Timestamp::now()).await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound for cross-tenant access, got {result:?}"
        );
    }
}
