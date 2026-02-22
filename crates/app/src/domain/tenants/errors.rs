//! Tenants service errors.

use sqlx::{
    Error,
    error::{DatabaseError, ErrorKind},
};
use thiserror::Error;

/// Tenant service error variants.
#[derive(Debug, Error)]
pub enum TenantsServiceError {
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

impl From<Error> for TenantsServiceError {
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
