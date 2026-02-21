//! Products service errors.

use std::num::TryFromIntError;

use sqlx::{
    Error,
    error::{DatabaseError, ErrorKind},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProductsServiceError {
    #[error("product already exists")]
    AlreadyExists,

    #[error("product not found")]
    NotFound,

    #[error("related resource not found")]
    InvalidReference,

    #[error("missing required data")]
    MissingRequiredData,

    #[error("invalid data")]
    InvalidData,

    #[error("storage error")]
    Sql(#[source] Error),

    #[error("invalid price value")]
    InvalidPrice(#[from] TryFromIntError),
}

impl From<Error> for ProductsServiceError {
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_price_rejects_negative() {
        let result = u64::try_from(-1_i64);

        assert!(result.is_err());
    }
}
