//! Request-level logging, request IDs, and parent trace extraction.

use std::time::Instant;

use opentelemetry::{global, propagation::Extractor, trace::TraceContextExt as _};
use salvo::{
    Request, handler,
    http::{HeaderMap, HeaderName, StatusCode, header::HeaderValue},
    prelude::{Depot, FlowCtrl, Response},
};
use tracing::{error, info, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use uuid::Uuid;

use super::settings;

const REQUEST_ID_HEADER: &str = "x-request-id";
const REQUEST_ID_DEPOT_KEY: &str = "request_id";

#[handler]
pub(crate) async fn request_logging(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let started = Instant::now();

    let request_id = req
        .header::<String>(REQUEST_ID_HEADER)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(generate_request_id);

    depot.insert(REQUEST_ID_DEPOT_KEY, request_id.clone());

    set_request_id_header(res, &request_id);

    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let remote_addr = req.remote_addr().to_string();

    let span = tracing::info_span!(
        parent: None,
        "http.request",
        request_id = %request_id,
        method = %method,
        path = %path,
        remote_addr = %remote_addr,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty
    );

    if settings::otel_parent_propagation_enabled()
        && let Some(parent_context) = extract_parent_context(req.headers())
        && let Err(source) = span.set_parent(parent_context)
    {
        warn!("failed to set parent context on request span: {source}");
    }

    let _enter = span.enter();

    ctrl.call_next(req, depot, res).await;

    let duration = started.elapsed();
    let status = res.status_code.unwrap_or(StatusCode::OK);
    let duration_ms = duration.as_millis();
    let threshold_ms = u128::from(settings::slow_request_threshold_ms());

    tracing::Span::current().record("status", status.as_u16());
    tracing::Span::current().record("duration_ms", duration_ms);

    info!(status = status.as_u16(), duration_ms, "request.completed");

    if status.is_server_error() {
        error!(
            status = status.as_u16(),
            method = %method,
            path = %path,
            request_id = %request_id,
            "server error response"
        );
    } else if status.is_client_error() {
        warn!(
            status = status.as_u16(),
            method = %method,
            path = %path,
            request_id = %request_id,
            "client error response"
        );
    }

    if duration_ms > threshold_ms {
        warn!(
            method = %method,
            path = %path,
            request_id = %request_id,
            duration_ms,
            threshold_ms,
            "slow request detected"
        );
    }
}

fn extract_parent_context(headers: &HeaderMap) -> Option<opentelemetry::Context> {
    let extractor = HeaderExtractor::new(headers);

    global::get_text_map_propagator(|propagator| {
        // Use a fresh base context so missing trace headers don't inherit the
        // currently active in-process span chain.
        let context = propagator.extract_with_context(&opentelemetry::Context::new(), &extractor);
        let span = context.span();
        let span_context = span.span_context();

        span_context.is_valid().then_some(context)
    })
}

fn set_request_id_header(res: &mut Response, request_id: &str) {
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

fn generate_request_id() -> String {
    Uuid::now_v7().to_string()
}

#[derive(Debug)]
struct HeaderExtractor<'a> {
    headers: &'a HeaderMap,
}

impl<'a> HeaderExtractor<'a> {
    fn new(headers: &'a HeaderMap) -> Self {
        Self { headers }
    }
}

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let value = self.headers.get(key)?;

        value.to_str().ok()
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(HeaderName::as_str).collect()
    }
}
