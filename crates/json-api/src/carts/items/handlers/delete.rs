//! Delete Cart Item Handler

use std::sync::Arc;

use salvo::{oapi::extract::PathParam, prelude::*};
use uuid::Uuid;

use crate::{carts::errors::into_status_error, extensions::*, state::State};

/// Delete Cart Item Handler
#[endpoint(
    tags("carts"),
    summary = "Delete Cart Item",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::OK, description = "Cart item deleted"),
        (status_code = StatusCode::NOT_FOUND, description = "Cart item not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    )
)]
pub(crate) async fn handler(
    cart: PathParam<Uuid>,
    item: PathParam<Uuid>,
    depot: &mut Depot,
) -> Result<StatusCode, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;

    state
        .app
        .carts
        .remove_item(tenant, cart.into_inner(), item.into_inner())
        .await
        .map_err(into_status_error)?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use salvo::test::TestClient;
    use testresult::TestResult;

    use lattice_app::domain::carts::{CartsServiceError, MockCartsService};

    use crate::test_helpers::{TEST_TENANT_UUID, carts_service, make_cart};

    use super::*;

    fn make_service(repo: MockCartsService) -> Service {
        carts_service(
            repo,
            Router::with_path("carts/{cart}/items/{item}").delete(handler),
        )
    }

    #[tokio::test]
    async fn test_delete_cart_success() -> TestResult {
        let cart = Uuid::now_v7();
        let item = Uuid::now_v7();

        make_cart(cart);

        let mut repo = MockCartsService::new();

        repo.expect_remove_item()
            .once()
            .withf(move |tenant, c, i| *tenant == TEST_TENANT_UUID && *c == cart && *i == item)
            .return_once(move |_, _, _| Ok(()));

        repo.expect_get_cart().never();
        repo.expect_create_cart().never();

        let res = TestClient::delete(format!("http://example.com/carts/{cart}/items/{item}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_cart_invalid_uuid_returns_400() -> TestResult {
        let mut repo = MockCartsService::new();

        repo.expect_get_cart().never();
        repo.expect_create_cart().never();
        repo.expect_remove_item().never();

        let res = TestClient::delete("http://example.com/carts/not-a-uuid/items/also-not-a-uuid")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_cart_not_found_returns_404() -> TestResult {
        let cart = Uuid::now_v7();
        let item = Uuid::now_v7();

        let mut repo = MockCartsService::new();

        repo.expect_remove_item()
            .once()
            .withf(move |tenant, c, i| *tenant == TEST_TENANT_UUID && *c == cart && *i == item)
            .return_once(|_, _, _| Err(CartsServiceError::NotFound));

        repo.expect_create_cart().never();
        repo.expect_get_cart().never();

        let res = TestClient::delete(format!("http://example.com/carts/{cart}/items/{item}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_FOUND));

        Ok(())
    }
}
