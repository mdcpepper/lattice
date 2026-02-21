//! Create Product Handler

use std::sync::Arc;

use salvo::{
    http::header::LOCATION,
    oapi::{ToSchema, extract::JsonBody},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{extensions::*, products::models::NewProduct, state::State};

/// Create Product Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct CreateProductRequest {
    pub uuid: Uuid,
    pub price: u64,
}

impl From<JsonBody<CreateProductRequest>> for NewProduct {
    fn from(json: JsonBody<CreateProductRequest>) -> Self {
        let request = json.into_inner();

        NewProduct {
            uuid: request.uuid,
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
        .products
        .create_product(tenant, json.into())
        .await
        .map_err(StatusError::from)?
        .uuid;

    res.add_header(LOCATION, format!("/products/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::CREATED);

    Ok(Json(ProductCreatedResponse { uuid }))
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use testresult::TestResult;

    use crate::{
        products::{MockProductsRepository, ProductsRepositoryError},
        test_helpers::{TEST_TENANT_UUID, products_service},
    };

    use super::{super::tests::*, *};

    fn make_service(repo: MockProductsRepository) -> Service {
        products_service(repo, Router::with_path("products").post(handler))
    }

    #[tokio::test]
    async fn test_create_product_success() -> TestResult {
        let uuid = Uuid::now_v7();
        let product = make_product(uuid);

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(move |_, _| Ok(product));

        repo.expect_get_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let mut res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        let body: ProductCreatedResponse = res.take_json().await?;
        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::CREATED));
        assert_eq!(location, Some(format!("/products/{uuid}").as_str()));
        assert_eq!(body.uuid, uuid);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_conflict_returns_409() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(|_, _| Err(ProductsRepositoryError::AlreadyExists));

        repo.expect_get_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::CONFLICT));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_invalid_price_returns_400() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID && *new == NewProduct { uuid, price: 100 }
            })
            .return_once(|_, _| Err(ProductsRepositoryError::InvalidData));

        repo.expect_get_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
