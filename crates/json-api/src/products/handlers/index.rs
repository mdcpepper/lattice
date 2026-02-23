//! Product Index Handler

use std::sync::Arc;

use salvo::{
    oapi::{ToSchema, extract::QueryParam},
    prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{extensions::*, products::get::ProductResponse, state::State};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductsResponse {
    /// The list of products
    pub products: Vec<ProductResponse>,
}

/// Product Index Handler
///
/// Returns a list of products.
#[endpoint(
    tags("products"),
    summary = "List Products",
    security(("bearer_auth" = []))
)]
pub(crate) async fn handler(
    at: QueryParam<String, false>,
    depot: &mut Depot,
) -> Result<Json<ProductsResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let point_in_time = at.into_point_in_time()?;

    let products = state
        .app
        .products
        .list_products(tenant, point_in_time)
        .await
        .or_500("failed to fetch products")?;

    Ok(Json(ProductsResponse {
        products: products.into_iter().map(Into::into).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use salvo::test::{ResponseExt, TestClient};
    use testresult::TestResult;

    use lattice_app::domain::products::{
        MockProductsService, ProductsServiceError,
        records::{ProductRecord, ProductUuid},
    };

    use crate::test_helpers::{TEST_TENANT_UUID, products_service};

    use super::*;

    fn make_product(uuid: ProductUuid, price: u64) -> ProductRecord {
        ProductRecord {
            uuid,
            price,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    fn make_service(repo: MockProductsService) -> Service {
        products_service(repo, Router::with_path("products").get(handler))
    }

    #[tokio::test]
    async fn test_index_returns_200() -> TestResult {
        let mut repo = MockProductsService::new();

        repo.expect_list_products()
            .once()
            .withf(|tenant, _| *tenant == TEST_TENANT_UUID)
            .return_once(|_, _| Ok(vec![]));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_index_returns_empty_list() -> TestResult {
        let mut repo = MockProductsService::new();

        repo.expect_list_products()
            .once()
            .withf(|tenant, _| *tenant == TEST_TENANT_UUID)
            .return_once(|_, _| Ok(vec![]));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let response: ProductsResponse = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await
            .take_json()
            .await?;

        assert!(response.products.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_index_returns_products() -> TestResult {
        let uuid_b = ProductUuid::new();
        let uuid_a = ProductUuid::new();

        let mut repo = MockProductsService::new();

        repo.expect_list_products()
            .once()
            .withf(|tenant, _| *tenant == TEST_TENANT_UUID)
            .return_once(move |_, _| {
                Ok(vec![make_product(uuid_a, 100), make_product(uuid_b, 200)])
            });

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let response: ProductsResponse = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await
            .take_json()
            .await?;

        assert_eq!(response.products.len(), 2, "expected two products");
        assert_eq!(response.products[0].uuid, uuid_a.into_uuid());
        assert_eq!(response.products[1].uuid, uuid_b.into_uuid());

        Ok(())
    }

    #[tokio::test]
    async fn test_index_repository_error_returns_500() -> TestResult {
        let mut repo = MockProductsService::new();

        repo.expect_list_products()
            .once()
            .withf(|tenant, _| *tenant == TEST_TENANT_UUID)
            .return_once(|_, _| Err(ProductsServiceError::InvalidData));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));

        Ok(())
    }

    #[tokio::test]
    async fn test_index_forwards_point_in_time_query_param() -> TestResult {
        let mut repo = MockProductsService::new();
        let at: Timestamp = "2026-02-21T12:00:00Z".parse()?;

        repo.expect_list_products()
            .once()
            .withf(move |tenant, point_in_time| *tenant == TEST_TENANT_UUID && *point_in_time == at)
            .return_once(|_, _| Ok(vec![]));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get("http://example.com/products?at=2026-02-21T12:00:00Z")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }
}
