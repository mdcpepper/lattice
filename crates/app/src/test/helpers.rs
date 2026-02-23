//! Test Helpers

use jiff::Timestamp;
use uuid::Uuid;

use crate::{
    domain::{
        carts::{
            CartsService, CartsServiceError,
            models::{Cart, CartItem, NewCart, NewCartItem},
        },
        products::{
            ProductsService, ProductsServiceError,
            models::{NewProduct, Product},
        },
        tenants::models::TenantUuid,
    },
    test::TestContext,
};

pub(crate) async fn add_item(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: Uuid,
    product: Uuid,
    item: Uuid,
) -> Result<CartItem, CartsServiceError> {
    ctx.carts
        .add_item(
            tenant,
            cart,
            NewCartItem {
                uuid: item,
                product_uuid: product,
            },
        )
        .await
}

pub(crate) async fn remove_item(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: Uuid,
    item: Uuid,
) -> Result<(), CartsServiceError> {
    ctx.carts.remove_item(tenant, cart, item).await
}

pub(crate) async fn get_cart(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: Uuid,
    point_in_time: Timestamp,
) -> Result<Cart, CartsServiceError> {
    ctx.carts.get_cart(tenant, cart, point_in_time).await
}

pub(crate) async fn create_cart(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: Uuid,
) -> Result<Cart, CartsServiceError> {
    ctx.carts.create_cart(tenant, NewCart { uuid: cart }).await
}

pub(crate) async fn create_product(
    ctx: &TestContext,
    tenant: TenantUuid,
    product: Uuid,
    price: u64,
) -> Result<Product, ProductsServiceError> {
    ctx.products
        .create_product(
            tenant,
            NewProduct {
                uuid: product,
                price,
            },
        )
        .await
}
