//! Result helper extensions for HTTP handlers.

use std::fmt::Display;

use salvo::prelude::StatusError;
use tracing::error;

/// Map any error to a logged internal server error.
pub(crate) trait ResultExt<T> {
    fn or_500(self, context: &str) -> Result<T, StatusError>;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Display,
{
    fn or_500(self, context: &str) -> Result<T, StatusError> {
        self.map_err(|error| {
            error!("{context}: {error}");

            StatusError::internal_server_error()
        })
    }
}
