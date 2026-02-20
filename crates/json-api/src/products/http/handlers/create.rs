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

/// Create Product Response
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
        (status_code = 201, description = "Product created"),
        (status_code = 409, description = "Product already exists"),
        (status_code = 400, description = "Bad Request"),
        (status_code = 500, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    json: JsonBody<CreateProductRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<ProductCreatedResponse>, StatusError> {
    let uuid = depot
        .obtain_or_500::<Arc<State>>()?
        .products
        .create_product(json.into())
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
    use jiff::Timestamp;
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use testresult::TestResult;

    use crate::products::{MockProductsRepository, ProductsRepositoryError, models::Product};

    use super::*;

    fn make_product(uuid: Uuid) -> Product {
        Product {
            uuid,
            price: 100,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    fn make_service(repo: MockProductsRepository) -> Service {
        let state = Arc::new(State::new(Arc::new(repo)));

        let router = Router::new()
            .hoop(affix_state::inject(state))
            .push(Router::with_path("products").post(handler));

        Service::new(router)
    }

    #[tokio::test]
    async fn test_create_product_returns_201() -> TestResult {
        let uuid = Uuid::now_v7();
        let product = make_product(uuid);

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .return_once(move |_| Ok(product));

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::CREATED));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_returns_location_header() -> TestResult {
        let uuid = Uuid::now_v7();
        let product = make_product(uuid);

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .return_once(move |_| Ok(product));

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(location, Some(format!("/products/{uuid}").as_str()));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_returns_uuid_in_body() -> TestResult {
        let uuid = Uuid::now_v7();
        let product = make_product(uuid);

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .return_once(move |_| Ok(product));

        let response: ProductCreatedResponse = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await
            .take_json()
            .await?;

        assert_eq!(response.uuid, uuid);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_conflict_returns_409() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockProductsRepository::new();

        repo.expect_create_product()
            .once()
            .return_once(|_| Err(ProductsRepositoryError::AlreadyExists));

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
            .return_once(|_| Err(ProductsRepositoryError::InvalidData));

        let res = TestClient::post("http://example.com/products")
            .json(&json!({ "uuid": uuid, "price": 100 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
