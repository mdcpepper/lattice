//! Auth repository.

use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, PgPool, Postgres, Row, postgres::PgRow, query, query_as};
use uuid::Uuid;

use crate::{
    auth::{ActiveApiToken, ApiTokenMetadata, ApiTokenVersion, NewApiToken},
    tenants::models::TenantUuid,
};

const FIND_ACTIVE_API_TOKEN_BY_UUID_SQL: &str =
    include_str!("sql/find_active_api_token_by_uuid.sql");
const TOUCH_API_TOKEN_LAST_USED_SQL: &str = include_str!("sql/touch_api_token_last_used.sql");
const CREATE_API_TOKEN_SQL: &str = include_str!("sql/create_api_token.sql");
const REVOKE_API_TOKEN_SQL: &str = include_str!("sql/revoke_api_token.sql");
const LIST_API_TOKENS_BY_TENANT_SQL: &str = include_str!("sql/list_api_tokens_by_tenant.sql");

#[derive(Debug, Clone)]
pub struct PgAuthRepository {
    pool: PgPool,
}

impl PgAuthRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn find_active_api_token_by_uuid(
        &self,
        token_uuid: Uuid,
        token_version: ApiTokenVersion,
    ) -> Result<Option<ActiveApiToken>, sqlx::Error> {
        query_as::<Postgres, ActiveApiToken>(FIND_ACTIVE_API_TOKEN_BY_UUID_SQL)
            .bind(token_uuid)
            .bind(token_version.as_i16())
            .fetch_optional(&self.pool)
            .await
    }

    pub(crate) async fn touch_api_token_last_used(
        &self,
        token_uuid: Uuid,
    ) -> Result<(), sqlx::Error> {
        query(TOUCH_API_TOKEN_LAST_USED_SQL)
            .bind(token_uuid)
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    pub(crate) async fn create_api_token(
        &self,
        token: &NewApiToken,
    ) -> Result<ApiTokenMetadata, sqlx::Error> {
        query_as::<Postgres, ApiTokenMetadata>(CREATE_API_TOKEN_SQL)
            .bind(token.uuid)
            .bind(token.tenant_uuid.into_uuid())
            .bind(token.version.as_i16())
            .bind(&token.token_hash)
            .bind(token.expires_at.map(SqlxTimestamp::from))
            .fetch_one(&self.pool)
            .await
    }

    pub async fn revoke_api_token(
        &self,
        token_uuid: Uuid,
    ) -> Result<Option<ApiTokenMetadata>, sqlx::Error> {
        query_as::<Postgres, ApiTokenMetadata>(REVOKE_API_TOKEN_SQL)
            .bind(token_uuid)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn list_api_tokens_by_tenant(
        &self,
        tenant_uuid: TenantUuid,
    ) -> Result<Vec<ApiTokenMetadata>, sqlx::Error> {
        query_as::<Postgres, ApiTokenMetadata>(LIST_API_TOKENS_BY_TENANT_SQL)
            .bind(tenant_uuid.into_uuid())
            .fetch_all(&self.pool)
            .await
    }
}

impl<'r> FromRow<'r, PgRow> for ActiveApiToken {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let version = parse_version(row.try_get::<i16, _>("version")?)?;

        Ok(Self {
            tenant_uuid: row.try_get::<Uuid, _>("tenant_uuid")?.into(),
            version,
            token_hash: row.try_get("token_hash")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for ApiTokenMetadata {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let version = parse_version(row.try_get::<i16, _>("version")?)?;

        Ok(Self {
            uuid: row.try_get("uuid")?,
            tenant_uuid: row.try_get::<Uuid, _>("tenant_uuid")?.into(),
            version,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            last_used_at: row
                .try_get::<Option<SqlxTimestamp>, _>("last_used_at")?
                .map(SqlxTimestamp::to_jiff),
            expires_at: row
                .try_get::<Option<SqlxTimestamp>, _>("expires_at")?
                .map(SqlxTimestamp::to_jiff),
            revoked_at: row
                .try_get::<Option<SqlxTimestamp>, _>("revoked_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}

fn parse_version(value: i16) -> Result<ApiTokenVersion, sqlx::Error> {
    ApiTokenVersion::try_from(value).map_err(|source| sqlx::Error::ColumnDecode {
        index: "version".to_string(),
        source: Box::new(source),
    })
}
