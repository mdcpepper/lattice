//! Promotions service errors.

use sqlx::{
    Error,
    error::{DatabaseError, ErrorKind},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromotionsServiceError {
    #[error("promotion already exists")]
    AlreadyExists,

    #[error("promotion not found")]
    NotFound,

    #[error("related resource not found")]
    InvalidReference,

    #[error("missing required data")]
    MissingRequiredData,

    #[error("invalid data")]
    InvalidData,

    #[error("storage error")]
    Sql(#[source] Error),
}

impl From<Error> for PromotionsServiceError {
    fn from(error: Error) -> Self {
        if matches!(error, Error::RowNotFound) {
            return Self::NotFound;
        }

        match error.as_database_error().map(DatabaseError::kind) {
            Some(ErrorKind::UniqueViolation) => Self::AlreadyExists,
            Some(ErrorKind::ForeignKeyViolation) => Self::InvalidReference,
            Some(ErrorKind::NotNullViolation) => Self::MissingRequiredData,
            Some(ErrorKind::CheckViolation) => Self::InvalidData,
            _ => Self::Sql(error),
        }
    }
}
