//! Create Cart Handler

use std::sync::Arc;

use salvo::{
    http::header::LOCATION,
    oapi::{ToSchema, extract::JsonBody},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lattice_app::domain::carts::models::NewCart;

use crate::{carts::errors::into_status_error, extensions::*, state::State};

/// Create Cart Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CreateCartRequest {
    pub uuid: Uuid,
}

impl From<CreateCartRequest> for NewCart {
    fn from(request: CreateCartRequest) -> Self {
        NewCart { uuid: request.uuid }
    }
}

/// Cart Created Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CartCreatedResponse {
    /// Created cart UUID
    pub uuid: Uuid,
}

/// Create Cart Handler
#[endpoint(
    tags("carts"),
    summary = "Create Cart",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::CREATED, description = "Cart created"),
        (status_code = StatusCode::CONFLICT, description = "Cart already exists"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    json: JsonBody<CreateCartRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<CartCreatedResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;

    let uuid = state
        .app
        .carts
        .create_cart(tenant, json.into_inner().into())
        .await
        .map_err(into_status_error)?
        .uuid;

    res.add_header(LOCATION, format!("/carts/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::CREATED);

    Ok(Json(CartCreatedResponse { uuid }))
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use testresult::TestResult;

    use lattice_app::domain::carts::{CartsServiceError, MockCartsService};

    use crate::test_helpers::{TEST_TENANT_UUID, carts_service, make_cart};

    use super::*;

    fn make_service(repo: MockCartsService) -> Service {
        carts_service(repo, Router::with_path("carts").post(handler))
    }

    #[tokio::test]
    async fn test_create_cart_success() -> TestResult {
        let uuid = Uuid::now_v7();
        let cart = make_cart(uuid);

        let mut repo = MockCartsService::new();

        repo.expect_create_cart()
            .once()
            .withf(move |tenant, new| *tenant == TEST_TENANT_UUID && *new == NewCart { uuid })
            .return_once(move |_, _| Ok(cart));

        repo.expect_get_cart().never();
        repo.expect_delete_cart().never();

        let mut res = TestClient::post("http://example.com/carts")
            .json(&json!({ "uuid": uuid }))
            .send(&make_service(repo))
            .await;

        let body: CartCreatedResponse = res.take_json().await?;
        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::CREATED));
        assert_eq!(location, Some(format!("/carts/{uuid}").as_str()));
        assert_eq!(body.uuid, uuid);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_cart_conflict_returns_409() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockCartsService::new();

        repo.expect_create_cart()
            .once()
            .withf(move |tenant, new| *tenant == TEST_TENANT_UUID && *new == NewCart { uuid })
            .return_once(|_, _| Err(CartsServiceError::AlreadyExists));

        repo.expect_get_cart().never();
        repo.expect_delete_cart().never();

        let res = TestClient::post("http://example.com/carts")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::CONFLICT));

        Ok(())
    }
}
