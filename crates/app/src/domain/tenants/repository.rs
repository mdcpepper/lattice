//! Tenants Repository

use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, PgPool, Postgres, Row, postgres::PgRow, query_as};

use crate::domain::tenants::{
    data::NewTenant,
    records::{TenantRecord, TenantUuid},
};

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

    pub(crate) async fn create_tenant(
        &self,
        tenant: NewTenant,
    ) -> Result<TenantRecord, sqlx::Error> {
        query_as::<Postgres, TenantRecord>(CREATE_TENANT_SQL)
            .bind(tenant.uuid.into_uuid())
            .bind(tenant.name)
            .fetch_one(&self.pool)
            .await
    }
}

impl<'r> FromRow<'r, PgRow> for TenantRecord {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            uuid: TenantUuid::from_uuid(row.try_get("uuid")?),
            name: row.try_get("name")?,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
