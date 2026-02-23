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
            models::{Cart, CartItem, NewCart, NewCartItem},
            repositories::{PgCartItemsRepository, PgCartsRepository},
        },
        tenants::models::TenantUuid,
    },
};

#[derive(Debug, Clone)]
pub struct PgCartsService {
    db: Db,
    carts_repository: PgCartsRepository,
    items_repository: PgCartItemsRepository,
}

impl PgCartsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            carts_repository: PgCartsRepository::new(),
            items_repository: PgCartItemsRepository::new(),
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

        let mut cart = self
            .carts_repository
            .get_cart(&mut tx, uuid, point_in_time)
            .await?;

        let items = self
            .items_repository
            .get_cart_items(&mut tx, uuid, point_in_time)
            .await?;

        tx.commit().await?;

        cart.items.extend(items);

        Ok(cart)
    }

    async fn create_cart(
        &self,
        tenant: TenantUuid,
        cart: NewCart,
    ) -> Result<Cart, CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let created = self
            .carts_repository
            .create_cart(&mut tx, cart.uuid)
            .await?;

        tx.commit().await?;

        Ok(created)
    }

    async fn delete_cart(&self, tenant: TenantUuid, uuid: Uuid) -> Result<(), CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.carts_repository.delete_cart(&mut tx, uuid).await?;

        if rows_affected == 0 {
            return Err(CartsServiceError::NotFound);
        }

        tx.commit().await?;

        Ok(())
    }

    async fn add_item(
        &self,
        tenant: TenantUuid,
        cart: Uuid,
        item: NewCartItem,
    ) -> Result<CartItem, CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let item = self
            .items_repository
            .create_cart_item(&mut tx, cart, item)
            .await?;

        tx.commit().await?;

        Ok(item)
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

    /// Add an item to the given cart
    async fn add_item(
        &self,
        tenant: TenantUuid,
        cart: Uuid,
        item: NewCartItem,
    ) -> Result<CartItem, CartsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use testresult::TestResult;
    use uuid::Uuid;

    use crate::{
        domain::{
            carts::models::NewCart,
            products::{ProductsService, models::NewProduct},
        },
        test::TestContext,
    };

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
        assert_eq!(cart.items.len(), 0);
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
    async fn delete_cart_makes_it_not_found() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await?;

        ctx.carts.delete_cart(ctx.tenant_uuid, uuid).await?;

        let result = ctx
            .carts
            .get_cart(ctx.tenant_uuid, uuid, Timestamp::now())
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound after deletion, got {result:?}"
        );

        Ok(())
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
    async fn cart_not_visible_to_other_tenant() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = Uuid::now_v7();

        let tenant_b = ctx.create_tenant("Tenant B").await;

        ctx.carts
            .create_cart(ctx.tenant_uuid, NewCart { uuid })
            .await?;

        let result = ctx.carts.get_cart(tenant_b, uuid, Timestamp::now()).await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound for cross-tenant access, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn adding_item_to_cart() -> TestResult {
        let ctx = TestContext::new().await;

        let product = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: Uuid::now_v7(),
                    price: 10_00,
                },
            )
            .await?;

        let cart = ctx
            .carts
            .create_cart(
                ctx.tenant_uuid,
                NewCart {
                    uuid: Uuid::now_v7(),
                },
            )
            .await?;

        let uuid = Uuid::now_v7();

        let item = ctx
            .carts
            .add_item(
                ctx.tenant_uuid,
                cart.uuid,
                NewCartItem {
                    uuid,
                    product_uuid: product.uuid,
                },
            )
            .await?;

        assert_eq!(item.uuid, uuid);
        assert_eq!(item.base_price, product.price);
        assert_eq!(item.product_uuid, product.uuid);
        assert!(item.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn adding_same_product_twice_creates_two_distinct_items() -> TestResult {
        let ctx = TestContext::new().await;

        let product = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: Uuid::now_v7(),
                    price: 10_00,
                },
            )
            .await?;

        let cart = ctx
            .carts
            .create_cart(
                ctx.tenant_uuid,
                NewCart {
                    uuid: Uuid::now_v7(),
                },
            )
            .await?;

        let uuid = Uuid::now_v7();

        let item_1 = ctx
            .carts
            .add_item(
                ctx.tenant_uuid,
                cart.uuid,
                NewCartItem {
                    uuid,
                    product_uuid: product.uuid,
                },
            )
            .await?;

        let item_2 = ctx
            .carts
            .add_item(
                ctx.tenant_uuid,
                cart.uuid,
                NewCartItem {
                    uuid: Uuid::now_v7(),
                    product_uuid: product.uuid,
                },
            )
            .await?;

        assert!(item_1.uuid != item_2.uuid);
        assert_eq!(item_1.product_uuid, item_2.product_uuid);

        Ok(())
    }

    #[tokio::test]
    async fn adding_item_with_unknown_product_returns_not_found() -> TestResult {
        let ctx = TestContext::new().await;

        let cart = ctx
            .carts
            .create_cart(
                ctx.tenant_uuid,
                NewCart {
                    uuid: Uuid::now_v7(),
                },
            )
            .await?;

        let result = ctx
            .carts
            .add_item(
                ctx.tenant_uuid,
                cart.uuid,
                NewCartItem {
                    uuid: Uuid::now_v7(),
                    product_uuid: Uuid::now_v7(),
                },
            )
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound for unknown product, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn item_not_added_to_other_tenants_cart() -> TestResult {
        let ctx = TestContext::new().await;

        let cart = ctx
            .carts
            .create_cart(
                ctx.tenant_uuid,
                NewCart {
                    uuid: Uuid::now_v7(),
                },
            )
            .await?;

        let product = ctx
            .products
            .create_product(
                ctx.tenant_uuid,
                NewProduct {
                    uuid: Uuid::now_v7(),
                    price: 10_00,
                },
            )
            .await?;

        let tenant_b = ctx.create_tenant("Tenant B").await;

        let result = ctx
            .carts
            .add_item(
                tenant_b,
                cart.uuid,
                NewCartItem {
                    uuid: Uuid::now_v7(),
                    product_uuid: product.uuid,
                },
            )
            .await;

        assert!(
            matches!(result, Err(CartsServiceError::NotFound)),
            "expected NotFound for cross-tenant insert, got {result:?}"
        );

        Ok(())
    }
}
