//! Tenants Repository

use async_trait::async_trait;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use mockall::automock;
use sqlx::{
    Error, FromRow, PgPool, Postgres, Row,
    error::{DatabaseError, ErrorKind},
    postgres::PgRow,
    query_as,
};
use thiserror::Error;

use crate::tenants::models::{NewTenant, Tenant};

const CREATE_TENANT_SQL: &str = include_str!("sql/create_tenant.sql");

/// Tenant repository error variants.
#[derive(Debug, Error)]
pub enum TenantsRepositoryError {
    /// Tenant already exists.
    #[error("tenant already exists")]
    AlreadyExists,

    /// Tenant was not found.
    #[error("tenant not found")]
    NotFound,

    /// Referenced related row does not exist.
    #[error("related resource not found")]
    InvalidReference,

    /// Required data was missing.
    #[error("missing required data")]
    MissingRequiredData,

    /// Provided data failed validation.
    #[error("invalid data")]
    InvalidData,

    /// Underlying SQL/storage error.
    #[error("storage error")]
    Sql(#[source] Error),
}

impl From<Error> for TenantsRepositoryError {
    fn from(error: Error) -> Self {
        if matches!(error, Error::RowNotFound) {
            return Self::NotFound;
        }

        match error.as_database_error().map(DatabaseError::kind) {
            Some(ErrorKind::UniqueViolation) => Self::AlreadyExists,
            Some(ErrorKind::ForeignKeyViolation) => Self::InvalidReference,
            Some(ErrorKind::NotNullViolation) => Self::MissingRequiredData,
            Some(ErrorKind::CheckViolation) => Self::InvalidData,
            Some(ErrorKind::Other | _) | None => Self::Sql(error),
        }
    }
}

#[derive(Debug, Clone)]
/// PostgreSQL-backed tenants repository.
pub struct PgTenantsRepository {
    pool: PgPool,
}

impl PgTenantsRepository {
    /// Creates a new repository instance.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

#[async_trait]
impl TenantsRepository for PgTenantsRepository {
    async fn create_tenant(&self, tenant: NewTenant) -> Result<Tenant, TenantsRepositoryError> {
        query_as::<Postgres, Tenant>(CREATE_TENANT_SQL)
            .bind(tenant.uuid)
            .bind(tenant.name)
            .bind(tenant.token_uuid)
            .bind(tenant.token_hash)
            .fetch_one(&self.pool)
            .await
            .map_err(TenantsRepositoryError::from)
    }
}

#[automock]
#[async_trait]
/// Tenant persistence operations.
pub trait TenantsRepository: Send + Sync {
    /// Creates a new tenant with the given name and token.
    async fn create_tenant(&self, tenant: NewTenant) -> Result<Tenant, TenantsRepositoryError>;
}
