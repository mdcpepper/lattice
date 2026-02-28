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
use smallvec::SmallVec;
use uuid::Uuid;

use lattice_app::domain::products::data::ProductUpdate;

use crate::{extensions::*, products::errors::into_status_error, state::State};

/// Update Product Request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct UpdateProductRequest {
    pub price: u64,
    #[serde(default)]
    pub tags: SmallVec<[String; 3]>,
}

impl From<UpdateProductRequest> for ProductUpdate {
    fn from(request: UpdateProductRequest) -> Self {
        ProductUpdate {
            uuid: None,
            price: request.price,
            tags: request.tags,
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
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::OK, description = "Product updated"),
        (status_code = StatusCode::NOT_FOUND, description = "Product not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
#[tracing::instrument(
    name = "products.update",
    skip(product, json, depot, res),
    fields(
        tenant_uuid = tracing::field::Empty,
        product_uuid = tracing::field::Empty,
        price = tracing::field::Empty,
        tags_count = tracing::field::Empty
    ),
    err
)]
pub(crate) async fn handler(
    product: PathParam<Uuid>,
    json: JsonBody<UpdateProductRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<ProductUpdatedResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let request = json.into_inner();
    let product = product.into_inner();

    let span = tracing::Span::current();

    span.record("tenant_uuid", tracing::field::display(tenant));
    span.record("product_uuid", tracing::field::display(product));
    span.record("price", tracing::field::display(request.price));
    span.record("tags_count", tracing::field::display(request.tags.len()));

    let price = state
        .app
        .products
        .update_product(tenant, product.into(), request.into())
        .await
        .map_err(into_status_error)?
        .price;

    res.add_header(LOCATION, format!("/products/{product}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::OK);

    tracing::info!(product_uuid = %product, price, "updated product");

    Ok(Json(ProductUpdatedResponse { price }))
}

#[cfg(test)]
mod tests {
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use smallvec::smallvec;
    use testresult::TestResult;

    use lattice_app::domain::products::{
        MockProductsService, ProductsServiceError, records::ProductUuid,
    };

    use crate::test_helpers::{TEST_TENANT_UUID, make_product, products_service};

    use super::*;

    fn make_service(repo: MockProductsService) -> Service {
        products_service(repo, Router::with_path("products/{product}").put(handler))
    }

    #[tokio::test]
    async fn test_update_product_success() -> TestResult {
        let uuid = ProductUuid::new();

        let mut product = make_product(uuid);

        product.price = 200;

        let mut repo = MockProductsService::new();

        repo.expect_update_product()
            .once()
            .withf(move |tenant, u, update| {
                *tenant == TEST_TENANT_UUID
                    && *u == uuid
                    && *update
                        == ProductUpdate {
                            uuid: None,
                            price: 200,
                            tags: smallvec![],
                        }
            })
            .return_once(move |_, _, _| Ok(product));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_list_products().never();
        repo.expect_delete_product().never();

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
    async fn test_update_product_invalid_uuid_returns_400() -> TestResult {
        let mut repo = MockProductsService::new();

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_list_products().never();
        repo.expect_update_product().never();
        repo.expect_delete_product().never();

        let res = TestClient::put("http://example.com/products/123")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }

    #[tokio::test]
    async fn test_update_product_invalid_price_returns_400() -> TestResult {
        let uuid = ProductUuid::new();

        let mut repo = MockProductsService::new();

        repo.expect_update_product()
            .once()
            .withf(move |tenant, u, update| {
                *tenant == TEST_TENANT_UUID
                    && *u == uuid
                    && *update
                        == ProductUpdate {
                            uuid: None,
                            price: 200,
                            tags: smallvec![],
                        }
            })
            .return_once(|_, _, _| Err(ProductsServiceError::InvalidData));

        repo.expect_get_product().never();
        repo.expect_create_product().never();
        repo.expect_list_products().never();
        repo.expect_delete_product().never();

        let res = TestClient::put(format!("http://example.com/products/{uuid}"))
            .json(&json!({ "price": 200 }))
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        Ok(())
    }
}
