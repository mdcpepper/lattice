//! Auth service errors.

use sqlx::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthServiceError {
    #[error("token not found")]
    NotFound,

    #[error("storage error")]
    Sql(#[source] Error),
}

impl From<Error> for AuthServiceError {
    fn from(error: Error) -> Self {
        Self::Sql(error)
    }
}
