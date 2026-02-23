//! App Router

use salvo::Router;

use crate::{auth, carts, products};

pub fn app_router() -> Router {
    Router::new()
        .hoop(auth::middleware::handler)
        .push(
            Router::with_path("carts")
                .post(carts::create::handler)
                .push(
                    Router::with_path("{cart}")
                        .get(carts::get::handler)
                        .delete(carts::delete::handler)
                        .push(Router::with_path("items").post(carts::items::create::handler)),
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
}
