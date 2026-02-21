//! State

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    auth::{AuthRepository, PgAuthRepository},
    products::{PgProductsRepository, ProductsRepository},
};

#[derive(Clone)]
pub(crate) struct State {
    pub(crate) products: Arc<dyn ProductsRepository>,
    pub(crate) auth: Arc<dyn AuthRepository>,
}

impl State {
    #[must_use]
    pub(crate) fn new(
        products: Arc<dyn ProductsRepository>,
        auth: Arc<dyn AuthRepository>,
    ) -> Self {
        Self { products, auth }
    }

    #[must_use]
    pub(crate) fn from_pool(pool: PgPool) -> Arc<Self> {
        Arc::new(Self::new(
            Arc::new(PgProductsRepository::new(pool.clone())),
            Arc::new(PgAuthRepository::new(pool)),
        ))
    }
}
