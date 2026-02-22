//! Auth service errors.

use sqlx::Error;
use thiserror::Error;

use crate::auth::{ApiTokenError, OpenBaoError};

#[derive(Debug, Error)]
pub enum AuthServiceError {
    #[error("token not found")]
    NotFound,

    #[error("storage error")]
    Sql(#[source] Error),

    #[error("token processing error")]
    Token(#[source] ApiTokenError),

    #[error("OpenBao error")]
    OpenBao(#[from] OpenBaoError),
}

impl From<Error> for AuthServiceError {
    fn from(error: Error) -> Self {
        Self::Sql(error)
    }
}

impl From<ApiTokenError> for AuthServiceError {
    fn from(error: ApiTokenError) -> Self {
        Self::Token(error)
    }
}
