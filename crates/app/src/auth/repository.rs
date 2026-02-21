//! Auth repository.

use sqlx::{FromRow, PgPool, Postgres, Row, postgres::PgRow, query_as};
use uuid::Uuid;

use crate::{auth::models::ApiToken, tenants::models::TenantUuid};

const FIND_TENANT_BY_TOKEN_HASH_SQL: &str = include_str!("sql/find_tenant_by_token_hash.sql");

#[derive(Debug, Clone)]
pub(crate) struct PgAuthRepository {
    pool: PgPool,
}

impl PgAuthRepository {
    #[must_use]
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn find_tenant_by_token_hash(
        &self,
        hash: &str,
    ) -> Result<Option<TenantUuid>, sqlx::Error> {
        query_as::<Postgres, ApiToken>(FIND_TENANT_BY_TOKEN_HASH_SQL)
            .bind(hash)
            .fetch_optional(&self.pool)
            .await
            .map(|record| record.map(|record| record.tenant_uuid))
    }
}

impl<'r> FromRow<'r, PgRow> for ApiToken {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            uuid: row.try_get("uuid")?,
            tenant_uuid: row.try_get::<Uuid, _>("tenant_uuid")?.into(),
            token_hash: row.try_get("token_hash")?,
        })
    }
}
