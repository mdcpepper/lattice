//! Test Helpers

use jiff::Timestamp;
use smallvec::SmallVec;

use crate::{
    domain::{
        carts::{
            CartsService, CartsServiceError,
            data::{NewCart, NewCartItem},
            records::{CartItemRecord, CartItemUuid, CartRecord, CartUuid},
        },
        products::{
            ProductsService, ProductsServiceError,
            data::NewProduct,
            records::{ProductRecord, ProductUuid},
        },
        tenants::records::TenantUuid,
    },
    test::TestContext,
};

pub(crate) async fn add_item(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: CartUuid,
    product: ProductUuid,
    item: CartItemUuid,
) -> Result<CartItemRecord, CartsServiceError> {
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
) -> Result<CartRecord, CartsServiceError> {
    ctx.carts.get_cart(tenant, cart, point_in_time).await
}

pub(crate) async fn create_cart(
    ctx: &TestContext,
    tenant: TenantUuid,
    cart: CartUuid,
) -> Result<CartRecord, CartsServiceError> {
    ctx.carts.create_cart(tenant, NewCart { uuid: cart }).await
}

pub(crate) async fn create_product(
    ctx: &TestContext,
    tenant: TenantUuid,
    product: ProductUuid,
    price: u64,
    tags: SmallVec<[String; 3]>,
) -> Result<ProductRecord, ProductsServiceError> {
    ctx.products
        .create_product(
            tenant,
            NewProduct {
                uuid: product,
                price,
                tags,
            },
        )
        .await
}
