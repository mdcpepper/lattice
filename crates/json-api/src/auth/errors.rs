//! Auth repository errors.

use sqlx::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AuthRepositoryError {
    #[error("token not found")]
    NotFound,

    #[error("storage error")]
    Sql(#[source] Error),
}

impl From<Error> for AuthRepositoryError {
    fn from(error: Error) -> Self {
        Self::Sql(error)
    }
}
