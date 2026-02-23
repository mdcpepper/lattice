//! Create Product Handler

use std::sync::Arc;

use salvo::{
    http::header::LOCATION,
    oapi::{ToSchema, extract::JsonBody},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lattice_app::domain::products::data::NewProduct;

use crate::{extensions::*, products::errors::into_status_error, state::State};

/// Create Product Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CreateProductRequest {
    pub uuid: Uuid,
    pub price: u64,
}

impl From<CreateProductRequest> for NewProduct {
    fn from(request: CreateProductRequest) -> Self {
        NewProduct {
            uuid: request.uuid.into(),
            price: request.price,
        }
    }
}

/// Product Created Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductCreatedResponse {
    /// Created product UUID
    pub uuid: Uuid,
}

/// Create Product Handler
#[endpoint(
    tags("products"),
    summary = "Create Product",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::CREATED, description = "Product created"),
        (status_code = StatusCode::CONFLICT, description = "Product already exists"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    json: JsonBody<CreateProductRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<ProductCreatedResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;

    let uuid = state
        .app
        .products
        .create_product(tenant, json.into_inner().into())
        .await
        .map_err(into_status_error)?
        .uuid;

    res.add_header(LOCATION, format!("/products/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::CREATED);

    Ok(Json(ProductCreatedResponse { uuid: uuid.into() }))
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use testresult::TestResult;

    use lattice_app::domain::products::{
        MockProductsService, ProductsServiceError, records::ProductUuid,
    };

    use crate::test_helpers::{TEST_TENANT_UUID, make_product, products_service};

    use super::*;

    fn make_service(repo: MockProductsService) -> Service {
        products_service(repo, Router::with_path("products").post(handler))
    }

    #[tokio::test]
    async fn test_create_product_success() -> TestResult {
        let uuid = ProductUuid::new();
        let product = make_product(uuid);

        let mut repo = MockProductsService::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(move |_, _| Ok(product));

        repo.expect_get_product().never();
        repo.expect_list_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let mut res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid.into_uuid(), "price": 100 }))
            .send(&make_service(repo))
            .await;

        let body: ProductCreatedResponse = res.take_json().await?;
        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::CREATED));
        assert_eq!(location, Some(format!("/products/{uuid}").as_str()));
        assert_eq!(body.uuid, uuid.into_uuid());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_conflict_returns_409() -> TestResult {
        let uuid = ProductUuid::new();

        let mut repo = MockProductsService::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(|_, _| Err(ProductsServiceError::AlreadyExists));

        repo.expect_get_product().never();
        repo.expect_list_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid.into_uuid(), "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::CONFLICT));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_invalid_price_returns_400() -> TestResult {
        let uuid = ProductUuid::new();

        let mut repo = MockProductsService::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(|_, _| Err(ProductsServiceError::InvalidData));

        repo.expect_get_product().never();
        repo.expect_list_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid.into_uuid(), "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
