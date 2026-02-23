//! Get Cart Handler

use std::{string::ToString, sync::Arc};

use salvo::{
    oapi::{
        ToSchema,
        extract::{PathParam, QueryParam},
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lattice_app::domain::carts::models::{Cart, CartItem};

use crate::{carts::errors::into_status_error, extensions::*, state::State};

/// Cart Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CartResponse {
    /// The unique identifier of the cart
    pub uuid: Uuid,

    /// The items in the cart
    pub items: Vec<CartItemResponse>,

    /// The date and time the cart was created
    pub created_at: String,

    /// The date and time the cart was last updated
    pub updated_at: String,

    /// The date and time the cart was deleted
    pub deleted_at: Option<String>,
}

impl From<Cart> for CartResponse {
    fn from(cart: Cart) -> Self {
        CartResponse {
            uuid: cart.uuid,
            items: cart.items.into_iter().map(CartItemResponse::from).collect(),
            created_at: cart.created_at.to_string(),
            updated_at: cart.updated_at.to_string(),
            deleted_at: cart.deleted_at.as_ref().map(ToString::to_string),
        }
    }
}

/// Cart Item Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CartItemResponse {
    /// The unique identifier of the cart item
    pub uuid: Uuid,

    /// The base price of the cart item
    pub base_price: u64,

    /// The unique identifier of the product in the cart item
    pub product_uuid: Uuid,

    /// The date and time the cart was created
    pub created_at: String,

    /// The date and time the cart was last updated
    pub updated_at: String,

    /// The date and time the cart was deleted
    pub deleted_at: Option<String>,
}

impl From<CartItem> for CartItemResponse {
    fn from(cart_item: CartItem) -> Self {
        Self {
            uuid: cart_item.uuid,
            base_price: cart_item.base_price,
            product_uuid: cart_item.product_uuid,
            created_at: cart_item.created_at.to_string(),
            updated_at: cart_item.updated_at.to_string(),
            deleted_at: cart_item.deleted_at.as_ref().map(ToString::to_string),
        }
    }
}

/// Get Cart Handler
///
/// Returns a cart.
#[endpoint(
    tags("carts"),
    summary = "Get Cart",
    security(("bearer_auth" = []))
)]
pub(crate) async fn handler(
    cart: PathParam<Uuid>,
    at: QueryParam<String, false>,
    depot: &mut Depot,
) -> Result<Json<CartResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let point_in_time = at.into_point_in_time()?;

    let cart = state
        .app
        .carts
        .get_cart(tenant, cart.into_inner(), point_in_time)
        .await
        .map_err(into_status_error)?;

    Ok(Json(cart.into()))
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use salvo::test::TestClient;
    use testresult::TestResult;

    use lattice_app::domain::carts::{CartsServiceError, MockCartsService};

    use crate::test_helpers::{TEST_TENANT_UUID, carts_service, make_cart};

    use super::*;

    fn make_service(repo: MockCartsService) -> Service {
        carts_service(repo, Router::with_path("carts/{cart}").get(handler))
    }

    #[tokio::test]
    async fn test_get_returns_200() -> TestResult {
        let mut repo = MockCartsService::new();
        let uuid = Uuid::now_v7();

        let cart = make_cart(uuid);

        repo.expect_get_cart()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(move |_, _, _| Ok(cart));

        repo.expect_create_cart().never();
        repo.expect_delete_cart().never();

        let res = TestClient::get(format!("http://example.com/carts/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_missing_cart_returns_404() -> TestResult {
        let mut repo = MockCartsService::new();
        let uuid = Uuid::now_v7();

        repo.expect_get_cart()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(|_, _, _| Err(CartsServiceError::NotFound));

        repo.expect_create_cart().never();
        repo.expect_delete_cart().never();

        let res = TestClient::get(format!("http://example.com/carts/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_FOUND));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_invalid_data_returns_400() -> TestResult {
        let mut repo = MockCartsService::new();
        let uuid = Uuid::now_v7();

        repo.expect_get_cart()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(|_, _, _| Err(CartsServiceError::InvalidData));

        repo.expect_create_cart().never();
        repo.expect_delete_cart().never();

        let res = TestClient::get(format!("http://example.com/carts/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_forwards_point_in_time_query_param() -> TestResult {
        let mut repo = MockCartsService::new();
        let uuid = Uuid::now_v7();
        let at: Timestamp = "2026-02-21T12:00:00Z".parse()?;
        let cart = make_cart(uuid);

        repo.expect_get_cart()
            .once()
            .withf(move |tenant, u, point_in_time| {
                *tenant == TEST_TENANT_UUID && *u == uuid && *point_in_time == at
            })
            .return_once(move |_, _, _| Ok(cart));

        repo.expect_create_cart().never();
        repo.expect_delete_cart().never();

        let res = TestClient::get(format!(
            "http://example.com/carts/{uuid}?at=2026-02-21T12:00:00Z"
        ))
        .send(&make_service(repo))
        .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }
}
