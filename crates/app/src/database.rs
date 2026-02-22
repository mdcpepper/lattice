//! Database connection management

use sqlx::{PgPool, Postgres, Transaction, query, query_as};

use crate::tenants::models::TenantUuid;

/// SQL used to set tenant context for row-level security.
pub const SET_TENANT_CONTEXT_SQL: &str = "SELECT set_config('app.current_tenant_uuid', $1, true)";
const ROLE_RLS_FLAGS_SQL: &str = r#"
SELECT
    current_user::text AS role_name,
    rolsuper,
    rolbypassrls
FROM pg_roles
WHERE rolname = current_user
"#;

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

/// Validate that the current database role cannot bypass row-level security.
///
/// # Errors
///
/// Returns an error when role metadata cannot be read or when the role has
/// `SUPERUSER` or `BYPASSRLS`.
pub async fn ensure_rls_enforced_role(pool: &PgPool) -> Result<(), sqlx::Error> {
    let (role_name, is_superuser, bypasses_rls): (String, bool, bool) =
        query_as::<Postgres, (String, bool, bool)>(ROLE_RLS_FLAGS_SQL)
            .fetch_one(pool)
            .await?;

    if is_superuser || bypasses_rls {
        return Err(sqlx::Error::Protocol(format!(
            "database role `{role_name}` bypasses RLS (SUPERUSER={is_superuser}, BYPASSRLS={bypasses_rls}); use a non-superuser role without BYPASSRLS"
        )));
    }

    Ok(())
}
