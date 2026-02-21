//! Database connection management

use sqlx::{PgPool, Postgres, Transaction, query};

use crate::tenants::models::TenantUuid;

/// SQL used to set tenant context for row-level security.
pub const SET_TENANT_CONTEXT_SQL: &str = "SELECT set_config('app.current_tenant_uuid', $1, true)";

#[derive(Debug, Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Begin a transaction and set tenant context for RLS policies.
    ///
    /// # Errors
    ///
    /// Returns an error when starting the transaction or setting tenant context fails.
    pub async fn begin_tenant_transaction(
        &self,
        tenant: TenantUuid,
    ) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        query(SET_TENANT_CONTEXT_SQL)
            .bind(tenant.into_uuid().to_string())
            .execute(&mut *tx)
            .await?;

        Ok(tx)
    }
}

/// Connect to `PostgreSQL`.
///
/// # Errors
///
/// Returns an error if the connection cannot be established.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(database_url).await
}
