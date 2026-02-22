//! Errors

use salvo::http::StatusError;
use tracing::error;

use lattice_app::domain::carts::CartsServiceError;

pub(crate) fn into_status_error(error: CartsServiceError) -> StatusError {
    match error {
        CartsServiceError::AlreadyExists => StatusError::conflict().brief("Cart already exists"),
        CartsServiceError::InvalidReference
        | CartsServiceError::MissingRequiredData
        | CartsServiceError::InvalidData => {
            StatusError::bad_request().brief("Invalid cart payload")
        }
        CartsServiceError::Sql(source) => {
            error!("failed to create cart: {source}");

            StatusError::internal_server_error()
        }
        CartsServiceError::NotFound => {
            error!("cart not found");

            StatusError::not_found()
        }
    }
}
