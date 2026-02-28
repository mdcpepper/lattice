//! Request ID generation and response header helpers.

use salvo::{
    http::{StatusCode, header::HeaderValue},
    prelude::Response,
};
use tracing::warn;
use uuid::Uuid;

pub(super) const REQUEST_ID_HEADER: &str = "x-request-id";

pub(super) fn resolve_request_id(header_value: Option<String>) -> String {
    header_value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(generate_request_id)
}

pub(super) fn set_request_id_header(res: &mut Response, request_id: &str) {
    let header_value = match HeaderValue::from_str(request_id) {
        Ok(value) => value,
        Err(source) => {
            warn!(
                request_id,
                "could not encode request id for response header: {source}"
            );

            return;
        }
    };

    res.headers_mut().insert(REQUEST_ID_HEADER, header_value);
}

pub(super) fn response_status_or_ok(status_code: Option<StatusCode>) -> StatusCode {
    status_code.unwrap_or(StatusCode::OK)
}

fn generate_request_id() -> String {
    Uuid::now_v7().to_string()
}
