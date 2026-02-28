//! Create Cart Item Handler

use std::sync::Arc;

use salvo::{
    http::header::LOCATION,
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lattice_app::domain::carts::data::NewCartItem;

use crate::{carts::errors::into_status_error, extensions::*, state::State};

/// Create Cart Item Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CreateCartItemRequest {
    pub uuid: Uuid,
    pub product_uuid: Uuid,
}

impl From<CreateCartItemRequest> for NewCartItem {
    fn from(request: CreateCartItemRequest) -> Self {
        NewCartItem {
            uuid: request.uuid.into(),
            product_uuid: request.product_uuid.into(),
        }
    }
}

/// Cart Item Created Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CartItemCreatedResponse {
    /// Created cart item UUID
    pub uuid: Uuid,
}

/// Create Cart Item Handler
#[endpoint(
    tags("carts"),
    summary = "Add Item to Cart",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::CREATED, description = "Cart item created"),
        (status_code = StatusCode::NOT_FOUND, description = "Cart not found"),
        (status_code = StatusCode::NOT_FOUND, description = "Product not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
#[tracing::instrument(
    name = "carts.items.create",
    skip(cart, json, depot, res),
    fields(
        tenant_uuid = tracing::field::Empty,
        cart_uuid = tracing::field::Empty,
        item_uuid = tracing::field::Empty,
        product_uuid = tracing::field::Empty
    ),
    err
)]
pub(crate) async fn handler(
    cart: PathParam<Uuid>,
    json: JsonBody<CreateCartItemRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<CartItemCreatedResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let request = json.into_inner();

    let cart = cart.into_inner();

    let span = tracing::Span::current();

    span.record("tenant_uuid", tracing::field::display(tenant));
    span.record("cart_uuid", tracing::field::display(cart));
    span.record("item_uuid", tracing::field::display(request.uuid));
    span.record(
        "product_uuid",
        tracing::field::display(request.product_uuid),
    );

    let item = state
        .app
        .carts
        .add_item(tenant, cart.into(), request.into())
        .await
        .map_err(into_status_error)?
        .uuid;

    res.add_header(LOCATION, format!("/carts/{cart}/items/{item}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::CREATED);

    tracing::info!(cart_uuid = %cart, item_uuid = %item, "created cart item");

    Ok(Json(CartItemCreatedResponse { uuid: item.into() }))
}
