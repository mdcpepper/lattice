//! Product Errors

use salvo::http::StatusError;
use tracing::error;

use lattice_app::domain::products::ProductsServiceError;

pub(crate) fn into_status_error(error: ProductsServiceError) -> StatusError {
    match error {
        ProductsServiceError::AlreadyExists => {
            StatusError::conflict().brief("Product already exists")
        }
        ProductsServiceError::InvalidReference
        | ProductsServiceError::MissingRequiredData
        | ProductsServiceError::InvalidData => {
            StatusError::bad_request().brief("Invalid product payload")
        }
        ProductsServiceError::Sql(source) => {
            error!("failed to create product: {source}");

            StatusError::internal_server_error()
        }
        ProductsServiceError::NotFound => {
            error!("product not found");

            StatusError::not_found()
        }
    }
}
