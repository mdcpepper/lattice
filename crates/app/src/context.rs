//! App Context

use std::sync::Arc;

use thiserror::Error;

use crate::{
    auth::{AuthService, OpenBaoClient, PgAuthService},
    database::{self, Db},
    products::{PgProductsService, ProductsService},
};

#[derive(Debug, Error)]
pub enum AppInitError {
    #[error("failed to connect to database")]
    Database(#[source] sqlx::Error),
}

#[derive(Clone)]
pub struct AppContext {
    pub products: Arc<dyn ProductsService>,
    pub auth: Arc<dyn AuthService>,
}

impl AppContext {
    /// Build application context from a database URL.
    ///
    /// # Errors
    ///
    /// Returns an error when establishing a database connection fails.
    pub async fn from_database_url(
        url: &str,
        openbao: OpenBaoClient,
    ) -> Result<Self, AppInitError> {
        let pool = database::connect(url)
            .await
            .map_err(AppInitError::Database)?;

        database::ensure_rls_enforced_role(&pool)
            .await
            .map_err(AppInitError::Database)?;

        let db = Db::new(pool.clone());

        Ok(Self {
            products: Arc::new(PgProductsService::new(db)),
            auth: Arc::new(PgAuthService::new(pool, openbao)),
        })
    }
}
