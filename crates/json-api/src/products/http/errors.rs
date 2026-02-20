//! Errors

use salvo::http::StatusError;
use tracing::error;

use crate::products::ProductsRepositoryError;

impl From<ProductsRepositoryError> for StatusError {
    fn from(error: ProductsRepositoryError) -> Self {
        match error {
            ProductsRepositoryError::AlreadyExists => {
                StatusError::conflict().brief("Product already exists")
            }
            ProductsRepositoryError::InvalidPrice(_) => {
                StatusError::bad_request().brief("Price is out of range")
            }
            ProductsRepositoryError::InvalidReference
            | ProductsRepositoryError::MissingRequiredData
            | ProductsRepositoryError::InvalidData => {
                StatusError::bad_request().brief("Invalid product payload")
            }
            ProductsRepositoryError::Sql(source) => {
                error!("failed to create product: {source}");

                StatusError::internal_server_error()
            }
        }
    }
}
