//! Promotion Errors

use salvo::http::StatusError;
use tracing::error;

use lattice_app::domain::promotions::PromotionsServiceError;

pub(crate) fn into_status_error(error: PromotionsServiceError) -> StatusError {
    match error {
        PromotionsServiceError::AlreadyExists => {
            StatusError::conflict().brief("Promotion already exists")
        }
        PromotionsServiceError::InvalidReference
        | PromotionsServiceError::MissingRequiredData
        | PromotionsServiceError::InvalidData => {
            StatusError::bad_request().brief("Invalid promotion payload")
        }
        PromotionsServiceError::Sql(source) => {
            error!("failed to process promotion: {source}");

            StatusError::internal_server_error()
        }
        PromotionsServiceError::NotFound => {
            error!("promotion not found");

            StatusError::not_found()
        }
    }
}
