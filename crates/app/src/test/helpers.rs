//! Test Helpers

use jiff::Timestamp;

use crate::{
    domain::{
        carts::{
            CartsService, CartsServiceError,
            models::{Cart, CartItem, CartItemUuid, CartUuid, NewCart, NewCartItem},
        },
        products::{
            ProductsService, ProductsServiceError,
            models::{NewProduct, Product, ProductUuid},
        },
        tenants::models::TenantUuid,
    },
    test::TestContext,
};

pub(crate) async fn add_item(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: CartUuid,
    product: ProductUuid,
    item: CartItemUuid,
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
    cart: CartUuid,
    item: CartItemUuid,
) -> Result<(), CartsServiceError> {
    ctx.carts.remove_item(tenant, cart, item).await
}

pub(crate) async fn get_cart(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: CartUuid,
    point_in_time: Timestamp,
) -> Result<Cart, CartsServiceError> {
    ctx.carts.get_cart(tenant, cart, point_in_time).await
}

pub(crate) async fn create_cart(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: CartUuid,
) -> Result<Cart, CartsServiceError> {
    ctx.carts.create_cart(tenant, NewCart { uuid: cart }).await
}

pub(crate) async fn create_product(
    ctx: &TestContext,
    tenant: TenantUuid,
    product: ProductUuid,
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
