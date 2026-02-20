//! Depot helper extensions.

use std::any::Any;

use salvo::prelude::{Depot, StatusError};

/// Helpers for mapping depot extraction failures to HTTP errors.
pub(crate) trait DepotExt {
    fn obtain_or_500<T: Any + Send + Sync>(&self) -> Result<&T, StatusError>;
}

impl DepotExt for Depot {
    fn obtain_or_500<T: Any + Send + Sync>(&self) -> Result<&T, StatusError> {
        self.obtain::<T>()
            .map_err(|_ignored| StatusError::internal_server_error())
    }
}
