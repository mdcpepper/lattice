//! Update Product Handler

use std::sync::Arc;

use salvo::{
    http::header::LOCATION,
    oapi::{
        ToSchema,
        extract::{JsonBody, PathParam},
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{extensions::*, products::models::ProductUpdate, state::State};

/// Update Product Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct UpdateProductRequest {
    pub price: u64,
}

impl From<JsonBody<UpdateProductRequest>> for ProductUpdate {
    fn from(json: JsonBody<UpdateProductRequest>) -> Self {
        let request = json.into_inner();

        ProductUpdate {
            price: request.price,
        }
    }
}

/// Product Updated Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductUpdatedResponse {
    /// Updated price
    pub price: u64,
}

/// Product Update Handler
#[endpoint(
    tags("products"),
    summary = "Update Product",
    responses(
        (status_code = StatusCode::OK, description = "Product updated"),
        (status_code = StatusCode::NOT_FOUND, description = "Product not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    uuid: PathParam<Uuid>,
    json: JsonBody<UpdateProductRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<ProductUpdatedResponse>, StatusError> {
    let uuid = uuid.into_inner();

    let price = depot
        .obtain_or_500::<Arc<State>>()?
        .products
        .update_product(uuid, json.into())
        .await
        .map_err(StatusError::from)?
        .price;

    res.add_header(LOCATION, format!("/products/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::OK);

    Ok(Json(ProductUpdatedResponse { price }))
}

#[cfg(test)]
mod tests {
    use salvo::{
        affix_state::inject,
        test::{ResponseExt, TestClient},
    };
    use serde_json::json;
    use testresult::TestResult;

    use crate::products::{MockProductsRepository, ProductsRepositoryError};

    use super::{super::tests::*, *};

    fn make_service(repo: MockProductsRepository) -> Service {
        let state = Arc::new(State::new(Arc::new(repo)));

        let router = Router::new()
            .hoop(inject(state))
            .push(Router::with_path("products/{uuid}").put(handler));

        Service::new(router)
    }

    #[tokio::test]
    async fn test_update_product_success() -> TestResult {
        let uuid = Uuid::now_v7();
        let mut product = make_product(uuid);
        product.price = 200;

        let mut repo = MockProductsRepository::new();

        repo.expect_update_product()
            .once()
            .withf(move |u, update| *u == uuid && *update == ProductUpdate { price: 200 })
            .return_once(move |_, _| Ok(product));

        let mut res = TestClient::put(format!("http://example.com/products/{uuid}"))
            .json(&json!({ "price": 200 }))
            .send(&make_service(repo))
            .await;

        let body: ProductUpdatedResponse = res.take_json().await?;
        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::OK));
        assert_eq!(location, Some(format!("/products/{uuid}").as_str()));
        assert_eq!(body.price, 200);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_product_invalid_price_returns_400() -> TestResult {
        let uuid = Uuid::now_v7();

        let mut repo = MockProductsRepository::new();

        repo.expect_update_product()
            .once()
            .withf(move |u, update| *u == uuid && *update == ProductUpdate { price: 200 })
            .return_once(|_, _| Err(ProductsRepositoryError::InvalidData));

        let res = TestClient::put(format!("http://example.com/products/{uuid}"))
            .json(&json!({ "price": 200 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
