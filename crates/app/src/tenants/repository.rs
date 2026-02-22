//! Tenants Repository

use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, PgPool, Postgres, Row, postgres::PgRow, query_as};

use crate::tenants::models::{NewTenant, Tenant};

const CREATE_TENANT_SQL: &str = include_str!("sql/create_tenant.sql");

#[derive(Debug, Clone)]
/// PostgreSQL-backed tenants repository.
pub(crate) struct PgTenantsRepository {
    pool: PgPool,
}

impl PgTenantsRepository {
    /// Creates a new repository instance.
    #[must_use]
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn create_tenant(&self, tenant: NewTenant) -> Result<Tenant, sqlx::Error> {
        query_as::<Postgres, Tenant>(CREATE_TENANT_SQL)
            .bind(tenant.uuid)
            .bind(tenant.name)
            .fetch_one(&self.pool)
            .await
    }
}

impl<'r> FromRow<'r, PgRow> for Tenant {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            uuid: row.try_get("uuid")?,
            name: row.try_get("name")?,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
