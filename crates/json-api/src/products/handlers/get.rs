//! Get Product Handler

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

use lattice_app::domain::products::records::ProductRecord;

use crate::{extensions::*, products::errors::into_status_error, state::State};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductResponse {
    /// The unique identifier of the product
    pub uuid: Uuid,

    /// The price of the product in pence/cents
    pub price: u64,

    /// The date and time the product was created
    pub created_at: String,

    /// The date and time the product was last updated
    pub updated_at: String,

    /// The date and time the product was deleted
    pub deleted_at: Option<String>,
}

impl From<ProductRecord> for ProductResponse {
    fn from(product: ProductRecord) -> Self {
        ProductResponse {
            uuid: product.uuid.into(),
            price: product.price,
            created_at: product.created_at.to_string(),
            updated_at: product.updated_at.to_string(),
            deleted_at: product.deleted_at.as_ref().map(ToString::to_string),
        }
    }
}

/// Get Product Handler
///
/// Returns a product.
#[endpoint(
    tags("products"),
    summary = "Get Product",
    security(("bearer_auth" = []))
)]
pub(crate) async fn handler(
    product: PathParam<Uuid>,
    at: QueryParam<String, false>,
    depot: &mut Depot,
) -> Result<Json<ProductResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let point_in_time = at.into_point_in_time()?;

    let product = state
        .app
        .products
        .get_product(tenant, product.into_inner().into(), point_in_time)
        .await
        .map_err(into_status_error)?;

    Ok(Json(product.into()))
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use salvo::test::TestClient;
    use testresult::TestResult;

    use lattice_app::domain::products::{
        MockProductsService, ProductsServiceError, records::ProductUuid,
    };

    use crate::test_helpers::{TEST_TENANT_UUID, make_product, products_service};

    use super::*;

    fn make_service(repo: MockProductsService) -> Service {
        products_service(repo, Router::with_path("products/{product}").get(handler))
    }

    #[tokio::test]
    async fn test_get_returns_200() -> TestResult {
        let mut repo = MockProductsService::new();
        let uuid = ProductUuid::new();

        let product = make_product(uuid);

        repo.expect_get_product()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(move |_, _, _| Ok(product));

        repo.expect_list_products().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get(format!("http://example.com/products/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_missing_product_returns_404() -> TestResult {
        let mut repo = MockProductsService::new();
        let uuid = ProductUuid::new();

        repo.expect_get_product()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(|_, _, _| Err(ProductsServiceError::NotFound));

        repo.expect_list_products().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get(format!("http://example.com/products/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_FOUND));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_invalid_data_returns_400() -> TestResult {
        let mut repo = MockProductsService::new();
        let uuid = ProductUuid::new();

        repo.expect_get_product()
            .once()
            .withf(move |tenant, u, _| *tenant == TEST_TENANT_UUID && *u == uuid)
            .return_once(|_, _, _| Err(ProductsServiceError::InvalidData));

        repo.expect_list_products().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get(format!("http://example.com/products/{uuid}"))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_forwards_point_in_time_query_param() -> TestResult {
        let mut repo = MockProductsService::new();
        let uuid = ProductUuid::new();
        let at: Timestamp = "2026-02-21T12:00:00Z".parse()?;
        let product = make_product(uuid);

        repo.expect_get_product()
            .once()
            .withf(move |tenant, u, point_in_time| {
                *tenant == TEST_TENANT_UUID && *u == uuid && *point_in_time == at
            })
            .return_once(move |_, _, _| Ok(product));

        repo.expect_list_products().never();
        repo.expect_create_product().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::get(format!(
            "http://example.com/products/{uuid}?at=2026-02-21T12:00:00Z"
        ))
        .send(&make_service(repo))
        .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }
}
