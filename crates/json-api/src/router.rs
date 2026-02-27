//! App Router

use salvo::Router;

use crate::{carts, products, promotions};

pub fn app_router() -> Router {
    Router::new()
        .push(
            Router::with_path("carts")
                .post(carts::create::handler)
                .push(
                    Router::with_path("{cart}")
                        .get(carts::get::handler)
                        .delete(carts::delete::handler)
                        .push(
                            Router::with_path("items")
                                .post(carts::items::create::handler)
                                .push(
                                    Router::with_path("{item}")
                                        .delete(carts::items::delete::handler),
                                ),
                        ),
                ),
        )
        .push(
            Router::with_path("products")
                .get(products::index::handler)
                .post(products::create::handler)
                .push(
                    Router::with_path("{product}")
                        .get(products::get::handler)
                        .put(products::update::handler)
                        .delete(products::delete::handler),
                ),
        )
        .push(
            Router::with_path("promotions")
                .post(promotions::create::handler)
                .push(Router::with_path("{uuid}").put(promotions::update::handler)),
        )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use salvo::{affix_state::inject, prelude::*, test::TestClient};
    use uuid::Uuid;

    use lattice_app::{
        auth::MockAuthService,
        context::AppContext,
        domain::{
            carts::{CartsServiceError, MockCartsService},
            products::{MockProductsService, ProductsServiceError},
            promotions::{PromotionsServiceError, service::MockPromotionsService},
        },
    };

    use crate::{state::State, test_helpers::inject_tenant};

    use super::app_router;

    fn router_service(
        carts: MockCartsService,
        products: MockProductsService,
        promotions: MockPromotionsService,
    ) -> Service {
        let state = Arc::new(State::new(AppContext {
            carts: Arc::new(carts),
            products: Arc::new(products),
            promotions: Arc::new(promotions),
            auth: Arc::new(MockAuthService::new()),
        }));

        Service::new(
            Router::new()
                .hoop(inject(state))
                .hoop(inject_tenant)
                .push(app_router()),
        )
    }

    #[tokio::test]
    async fn test_post_carts_is_registered() {
        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::post("http://example.com/carts")
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "POST /carts should be registered"
        );
    }

    #[tokio::test]
    async fn test_get_cart_is_registered() {
        let mut carts = MockCartsService::new();

        carts
            .expect_get_cart()
            .return_once(|_, _, _| Err(CartsServiceError::AlreadyExists));

        let service = router_service(
            carts,
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::get(format!("http://example.com/carts/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "GET /carts/{{cart}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_delete_cart_is_registered() {
        let mut carts = MockCartsService::new();
        carts
            .expect_delete_cart()
            .return_once(|_, _| Err(CartsServiceError::AlreadyExists));

        let service = router_service(
            carts,
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::delete(format!("http://example.com/carts/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "DELETE /carts/{{cart}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_post_cart_items_is_registered() {
        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::post(format!("http://example.com/carts/{}/items", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "POST /carts/{{cart}}/items should be registered"
        );
    }

    #[tokio::test]
    async fn test_delete_cart_item_is_registered() {
        let mut carts = MockCartsService::new();
        carts
            .expect_remove_item()
            .return_once(|_, _, _| Err(CartsServiceError::AlreadyExists));

        let service = router_service(
            carts,
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::delete(format!(
            "http://example.com/carts/{}/items/{}",
            Uuid::nil(),
            Uuid::nil()
        ))
        .send(&service)
        .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "DELETE /carts/{{cart}}/items/{{item}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_get_products_is_registered() {
        let mut products = MockProductsService::new();
        products
            .expect_list_products()
            .return_once(|_, _| Ok(vec![]));

        let service = router_service(
            MockCartsService::new(),
            products,
            MockPromotionsService::new(),
        );

        let res = TestClient::get("http://example.com/products")
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "GET /products should be registered"
        );
    }

    #[tokio::test]
    async fn test_post_products_is_registered() {
        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::post("http://example.com/products")
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "POST /products should be registered"
        );
    }

    #[tokio::test]
    async fn test_post_promotions_is_registered() {
        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::post("http://example.com/promotions")
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "POST /promotions should be registered"
        );
    }

    #[tokio::test]
    async fn test_put_promotion_is_registered() {
        let mut promotions = MockPromotionsService::new();

        promotions
            .expect_update_promotion()
            .return_once(|_, _, _| Err(PromotionsServiceError::NotFound));

        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            promotions,
        );

        let res = TestClient::put(format!("http://example.com/promotions/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "PUT /promotions/{{uuid}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_get_product_is_registered() {
        let mut products = MockProductsService::new();

        products
            .expect_get_product()
            .return_once(|_, _, _| Err(ProductsServiceError::AlreadyExists));

        let service = router_service(
            MockCartsService::new(),
            products,
            MockPromotionsService::new(),
        );

        let res = TestClient::get(format!("http://example.com/products/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "GET /products/{{product}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_put_product_is_registered() {
        let service = router_service(
            MockCartsService::new(),
            MockProductsService::new(),
            MockPromotionsService::new(),
        );

        let res = TestClient::put(format!("http://example.com/products/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "PUT /products/{{product}} should be registered"
        );
    }

    #[tokio::test]
    async fn test_delete_product_is_registered() {
        let mut products = MockProductsService::new();
        products
            .expect_delete_product()
            .return_once(|_, _| Err(ProductsServiceError::AlreadyExists));

        let service = router_service(
            MockCartsService::new(),
            products,
            MockPromotionsService::new(),
        );

        let res = TestClient::delete(format!("http://example.com/products/{}", Uuid::nil()))
            .send(&service)
            .await;

        assert_ne!(
            res.status_code,
            Some(StatusCode::NOT_FOUND),
            "DELETE /products/{{product}} should be registered"
        );
    }
}
