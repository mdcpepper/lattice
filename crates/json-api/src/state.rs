//! State

use std::sync::Arc;

use sqlx::PgPool;

use crate::products::{PgProductsRepository, ProductsRepository};

#[derive(Clone)]
pub(crate) struct State {
    pub(crate) products: Arc<dyn ProductsRepository>,
}

impl State {
    #[must_use]
    pub(crate) fn new(products: Arc<dyn ProductsRepository>) -> Self {
        Self { products }
    }

    #[must_use]
    pub(crate) fn from_pool(pool: PgPool) -> Arc<Self> {
        Arc::new(Self::new(Arc::new(PgProductsRepository::new(pool))))
    }
}
