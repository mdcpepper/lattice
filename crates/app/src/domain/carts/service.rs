//! Carts service.

use async_trait::async_trait;
use jiff::Timestamp;
use mockall::automock;

use crate::{
    database::Db,
    domain::{
        carts::{
            errors::CartsServiceError,
            models::{Cart, CartItem, CartItemUuid, CartUuid, NewCart, NewCartItem},
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
        cart: CartUuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let items = self
            .items_repository
            .get_cart_items(&mut tx, cart, point_in_time)
            .await?;

        let mut cart = self
            .carts_repository
            .get_cart(&mut tx, cart, point_in_time)
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

    async fn delete_cart(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
    ) -> Result<(), CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let rows_affected = self.carts_repository.delete_cart(&mut tx, cart).await?;

        if rows_affected == 0 {
            return Err(CartsServiceError::NotFound);
        }

        tx.commit().await?;

        Ok(())
    }

    async fn add_item(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
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

    async fn remove_item(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
        item: CartItemUuid,
    ) -> Result<(), CartsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let rows_affected = self
            .items_repository
            .delete_cart_item(&mut tx, cart, item)
            .await?;

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
        cart: CartUuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, CartsServiceError>;

    /// Creates a new cart with the given details.
    async fn create_cart(
        &self,
        tenant: TenantUuid,
        cart: NewCart,
    ) -> Result<Cart, CartsServiceError>;

    /// Deletes a cart with the given UUID.
    async fn delete_cart(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
    ) -> Result<(), CartsServiceError>;

    /// Add an item to the given cart
    async fn add_item(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
        item: NewCartItem,
    ) -> Result<CartItem, CartsServiceError>;

    /// Remove an item from the given cart
    async fn remove_item(
        &self,
        tenant: TenantUuid,
        cart: CartUuid,
        item: CartItemUuid,
    ) -> Result<(), CartsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use testresult::TestResult;

    use crate::{
        domain::carts::models::NewCart,
        test::{
            TestContext,
            helpers::{add_item, create_cart, create_product, get_cart, remove_item},
        },
    };

    use super::*;

    mod create {
        use super::*;

        #[tokio::test]
        async fn create_cart_returns_correct_uuid() -> TestResult {
            let ctx = TestContext::new().await;
            let uuid = CartUuid::new();

            let cart = ctx
                .carts
                .create_cart(ctx.tenant_uuid, NewCart { uuid })
                .await?;

            assert_eq!(cart.uuid, uuid);
            assert_eq!(cart.subtotal, 0);
            assert_eq!(cart.total, 0);
            assert!(cart.deleted_at.is_none());

            Ok(())
        }

        #[tokio::test]
        async fn create_cart_duplicate_uuid_returns_already_exists() -> TestResult {
            let ctx = TestContext::new().await;
            let uuid = CartUuid::new();

            ctx.carts
                .create_cart(ctx.tenant_uuid, NewCart { uuid })
                .await?;

            let result = ctx
                .carts
                .create_cart(ctx.tenant_uuid, NewCart { uuid })
                .await;

            assert!(
                matches!(result, Err(CartsServiceError::AlreadyExists)),
                "expected AlreadyExists, got {result:?}"
            );

            Ok(())
        }

        #[tokio::test]
        async fn cart_not_visible_to_other_tenant() -> TestResult {
            let ctx = TestContext::new().await;
            let uuid = CartUuid::new();

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
    }

    mod get {
        use crate::domain::products::models::ProductUuid;

        use super::*;

        #[tokio::test]
        async fn get_cart_returns_created_cart() -> TestResult {
            let ctx = TestContext::new().await;
            let uuid = CartUuid::new();

            create_cart(&ctx, ctx.tenant_uuid, uuid).await?;

            let cart = ctx
                .carts
                .get_cart(ctx.tenant_uuid, uuid, Timestamp::now())
                .await?;

            assert_eq!(cart.uuid, uuid);
            assert_eq!(cart.items.len(), 0);
            assert!(cart.deleted_at.is_none());

            Ok(())
        }

        #[tokio::test]
        async fn get_cart_returns_cart_with_items() -> TestResult {
            let ctx = TestContext::new().await;
            let cart_uuid = CartUuid::new();

            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;

            create_cart(&ctx, ctx.tenant_uuid, cart_uuid).await?;

            let item_uuid = CartItemUuid::new();

            add_item(&ctx, ctx.tenant_uuid, cart_uuid, product.uuid, item_uuid).await?;

            let cart = ctx
                .carts
                .get_cart(ctx.tenant_uuid, cart_uuid, Timestamp::now())
                .await?;

            assert_eq!(cart.uuid, cart_uuid);
            assert_eq!(cart.items.len(), 1);
            assert!(cart.deleted_at.is_none());

            let Some(item) = cart.items.first() else {
                panic!("expected item, got None");
            };

            assert_eq!(item.uuid, item_uuid);
            assert_eq!(item.product_uuid, product.uuid);

            Ok(())
        }

        #[tokio::test]
        async fn get_cart_unknown_uuid_returns_not_found() {
            let ctx = TestContext::new().await;

            let result = ctx
                .carts
                .get_cart(ctx.tenant_uuid, CartUuid::new(), Timestamp::now())
                .await;

            assert!(
                matches!(result, Err(CartsServiceError::NotFound)),
                "expected NotFound, got {result:?}"
            );
        }
    }

    mod delete {
        use super::*;

        #[tokio::test]
        async fn delete_cart_makes_it_not_found() -> TestResult {
            let ctx = TestContext::new().await;
            let uuid = CartUuid::new();

            create_cart(&ctx, ctx.tenant_uuid, uuid).await?;

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

            let result = ctx
                .carts
                .delete_cart(ctx.tenant_uuid, CartUuid::new())
                .await;

            assert!(
                matches!(result, Err(CartsServiceError::NotFound)),
                "expected NotFound, got {result:?}"
            );
        }
    }

    mod add_item {
        use crate::domain::products::models::ProductUuid;

        use super::*;

        #[tokio::test]
        async fn adding_item_to_cart() -> TestResult {
            let ctx = TestContext::new().await;

            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;

            let uuid = CartItemUuid::new();
            let item = add_item(&ctx, ctx.tenant_uuid, cart.uuid, product.uuid, uuid).await?;

            assert_eq!(item.uuid, uuid);
            assert_eq!(item.base_price, product.price);
            assert_eq!(item.product_uuid, product.uuid);
            assert!(item.deleted_at.is_none());

            Ok(())
        }

        #[tokio::test]
        async fn adding_same_product_twice_creates_two_distinct_items() -> TestResult {
            let ctx = TestContext::new().await;
            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;

            let uuid = CartItemUuid::new();

            let item_1 = add_item(&ctx, ctx.tenant_uuid, cart.uuid, product.uuid, uuid).await?;
            let item_2 = add_item(
                &ctx,
                ctx.tenant_uuid,
                cart.uuid,
                product.uuid,
                CartItemUuid::new(),
            )
            .await?;

            assert!(item_1.uuid != item_2.uuid);
            assert_eq!(item_1.product_uuid, item_2.product_uuid);

            Ok(())
        }

        #[tokio::test]
        async fn adding_item_with_unknown_product_returns_not_found() -> TestResult {
            let ctx = TestContext::new().await;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;

            let result = add_item(
                &ctx,
                ctx.tenant_uuid,
                cart.uuid,
                ProductUuid::new(),
                CartItemUuid::new(),
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
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;
            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;

            let tenant_b = ctx.create_tenant("Tenant B").await;

            let result =
                add_item(&ctx, tenant_b, cart.uuid, product.uuid, CartItemUuid::new()).await;

            assert!(
                matches!(result, Err(CartsServiceError::NotFound)),
                "expected NotFound for cross-tenant insert, got {result:?}"
            );

            Ok(())
        }
    }

    mod remove_item {
        use crate::domain::products::models::ProductUuid;

        use super::*;

        #[tokio::test]
        async fn remove_item_success() -> TestResult {
            let ctx = TestContext::new().await;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;
            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;

            let item = add_item(
                &ctx,
                ctx.tenant_uuid,
                cart.uuid,
                product.uuid,
                CartItemUuid::new(),
            )
            .await?;

            remove_item(&ctx, ctx.tenant_uuid, cart.uuid, item.uuid).await?;

            let cart = get_cart(&ctx, ctx.tenant_uuid, cart.uuid, Timestamp::now()).await?;

            assert!(cart.items.is_empty());

            Ok(())
        }

        #[tokio::test]
        async fn remove_item_not_exist_errors() -> TestResult {
            let ctx = TestContext::new().await;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;
            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;

            add_item(
                &ctx,
                ctx.tenant_uuid,
                cart.uuid,
                product.uuid,
                CartItemUuid::new(),
            )
            .await?;

            let result = remove_item(&ctx, ctx.tenant_uuid, cart.uuid, CartItemUuid::new()).await;

            assert!(
                matches!(result, Err(CartsServiceError::NotFound)),
                "expected NotFound for non-existent item, got {result:?}"
            );

            Ok(())
        }

        #[tokio::test]
        async fn item_not_removed_from_other_tenants_cart() -> TestResult {
            let ctx = TestContext::new().await;
            let cart = create_cart(&ctx, ctx.tenant_uuid, CartUuid::new()).await?;
            let product = create_product(&ctx, ctx.tenant_uuid, ProductUuid::new(), 10_00).await?;

            let item = add_item(
                &ctx,
                ctx.tenant_uuid,
                cart.uuid,
                product.uuid,
                CartItemUuid::new(),
            )
            .await?;

            let tenant_b = ctx.create_tenant("Tenant B").await;

            let result = remove_item(&ctx, tenant_b, cart.uuid, item.uuid).await;

            assert!(
                matches!(result, Err(CartsServiceError::NotFound)),
                "expected NotFound for non-existent item, got {result:?}"
            );

            Ok(())
        }
    }
}
