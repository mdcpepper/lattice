//! Product Index Handler

use std::{string::ToString, sync::Arc};

use salvo::{oapi::ToSchema, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{extensions::*, products::models::Product, state::State};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductResponse {
    /// The unique identifier of the product
    uuid: Uuid,

    /// The price of the product in pence/cents
    price: u64,

    /// The date and time the product was created
    created_at: String,

    /// The date and time the product was last updated
    updated_at: String,

    /// The date and time the product was deleted
    deleted_at: Option<String>,
}

impl From<Product> for ProductResponse {
    fn from(product: Product) -> Self {
        ProductResponse {
            uuid: product.uuid,
            price: product.price,
            created_at: product.created_at.to_string(),
            updated_at: product.updated_at.to_string(),
            deleted_at: product.deleted_at.as_ref().map(ToString::to_string),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct ProductsResponse {
    /// The list of products
    pub products: Vec<ProductResponse>,
}

/// Product Index Handler
///
/// Returns a list of products.
#[endpoint(tags("products"), summary = "List Products")]
pub(crate) async fn handler(depot: &mut Depot) -> Result<Json<ProductsResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;

    let products = state
        .products
        .get_products()
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

    use crate::products::{MockProductsRepository, ProductsRepositoryError};

    use super::*;

    fn make_product(uuid: Uuid, price: u64) -> Product {
        Product {
            uuid,
            price,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    fn make_service(repo: MockProductsRepository) -> Service {
        let state = Arc::new(State::new(Arc::new(repo)));

        let router = Router::new()
            .hoop(affix_state::inject(state))
            .push(Router::with_path("products").get(handler));

        Service::new(router)
    }

    #[tokio::test]
    async fn test_index_returns_200() -> TestResult {
        let mut repo = MockProductsRepository::new();

        repo.expect_get_products().once().return_once(|| Ok(vec![]));

        let res = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));

        Ok(())
    }

    #[tokio::test]
    async fn test_index_returns_empty_list() -> TestResult {
        let mut repo = MockProductsRepository::new();

        repo.expect_get_products().once().return_once(|| Ok(vec![]));

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
        let uuid_a = Uuid::now_v7();
        let uuid_b = Uuid::now_v7();

        let mut repo = MockProductsRepository::new();

        repo.expect_get_products()
            .once()
            .return_once(move || Ok(vec![make_product(uuid_a, 100), make_product(uuid_b, 200)]));

        let response: ProductsResponse = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await
            .take_json()
            .await?;

        assert_eq!(response.products.len(), 2, "expected two products");
        assert_eq!(response.products[0].uuid, uuid_a);
        assert_eq!(response.products[1].uuid, uuid_b);

        Ok(())
    }

    #[tokio::test]
    async fn test_index_repository_error_returns_500() -> TestResult {
        let mut repo = MockProductsRepository::new();

        repo.expect_get_products()
            .once()
            .return_once(|| Err(ProductsRepositoryError::InvalidData));

        let res = TestClient::get("http://example.com/products")
            .send(&make_service(repo))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));

        Ok(())
    }
}
