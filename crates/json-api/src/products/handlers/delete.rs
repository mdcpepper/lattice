//! Delete Product Handler

use std::sync::Arc;

use salvo::{oapi::extract::PathParam, prelude::*};
use uuid::Uuid;

use crate::{extensions::*, products::errors::into_status_error, state::State};

/// Delete Product Handler
#[endpoint(
    tags("products"),
    summary = "Delete Product",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::OK, description = "Product deleted"),
        (status_code = StatusCode::NOT_FOUND, description = "Product not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    uuid: PathParam<Uuid>,
    depot: &mut Depot,
) -> Result<StatusCode, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;

    state
        .app
        .products
        .delete_product(tenant, uuid.into_inner())
        .await
        .map_err(into_status_error)?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use salvo::test::TestClient;
    use testresult::TestResult;

    use lattice_app::products::{MockProductsService, ProductsServiceError};

    use crate::test_helpers::{TEST_TENANT_UUID, products_service};

    use super::{super::tests::*, *};

    fn make_service(repo: MockProductsService) -> Service {
        products_service(repo, Router::with_path("products/{uuid}").delete(handler))
    }

    #[tokio::test]
    async fn test_delete_product_success() -> TestResult {
        let uuid = Uuid::now_v7();

        make_product(uuid);

        let mut repo = MockProductsService::new();

        repo.expect_delete_product()
            .once()
            .withf(move |tenant, u| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(move |_, _| Ok(()));

        let res = TestClient::delete(format!("http://example.com/products/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_product_invalid_uuid_returns_400() -> TestResult {
        let res = TestClient::delete("http://example.com/products/123")
            .send(&make_service(MockProductsService::new()))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_product_not_found_returns_404() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockProductsService::new();

        repo.expect_delete_product()
            .once()
            .withf(move |tenant, u| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(|_, _| Err(ProductsServiceError::InvalidReference));

        repo.expect_create_product().never();
        repo.expect_get_products().never();
        repo.expect_update_product().never();

        let res = TestClient::delete(format!("http://example.com/products/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
