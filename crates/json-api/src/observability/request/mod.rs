//! Request-level logging, request IDs, and parent trace extraction.

mod parent_context;
mod request_ids;
mod spans;

use std::time::Instant;

use pyroscope::ThreadId;
use salvo::{
    Request, handler,
    prelude::{Depot, FlowCtrl, Response},
};
use tracing::Instrument as _;
use tracing::{error, info, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use super::{metrics, profiling, settings};

const REQUEST_ID_DEPOT_KEY: &str = "request_id";

#[handler]
pub(crate) async fn request_logging(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    if req.uri().path() == "/metrics" {
        ctrl.call_next(req, depot, res).await;
        return;
    }

    let started = Instant::now();

    let request_id =
        request_ids::resolve_request_id(req.header::<String>(request_ids::REQUEST_ID_HEADER));

    depot.insert(REQUEST_ID_DEPOT_KEY, request_id.clone());

    request_ids::set_request_id_header(res, &request_id);

    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let remote_addr = req.remote_addr().to_string();
    let names = spans::request_span_name(&method, &path);
    let otel_path = names.otel_path;
    let otel_span_name = names.otel_span_name;
    let request_thread_id = ThreadId::pthread_self();
    let _in_flight_request = metrics::InFlightRequestGuard::track();

    let span = tracing::info_span!(
        parent: None,
        "http.request",
        otel.name = %otel_span_name,
        otel.kind = "server",
        request_id = %request_id,
        method = %method,
        path = %path,
        remote_addr = %remote_addr,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty
    );

    if settings::otel_parent_propagation_enabled()
        && let Some(parent_context) = parent_context::extract_parent_context(req.headers())
        && let Err(source) = span.set_parent(parent_context)
    {
        warn!("failed to set parent context on request span: {source}");
    }

    if let Err(source) = profiling::add_request_tags(request_thread_id.clone(), &method, &otel_path)
    {
        warn!("failed to add pyroscope request tags: {source}");
    }

    ctrl.call_next(req, depot, res)
        .instrument(span.clone())
        .await;

    if let Err(source) = profiling::remove_request_tags(request_thread_id, &method, &otel_path) {
        warn!("failed to remove pyroscope request tags: {source}");
    }

    let duration = started.elapsed();
    let status = request_ids::response_status_or_ok(res.status_code);
    let duration_ms = duration.as_millis();
    let threshold_ms = u128::from(settings::slow_request_threshold_ms());

    metrics::observe_request(&method, &otel_path, status.as_u16(), duration.as_secs_f64());

    span.record("status", status.as_u16());
    span.record("duration_ms", duration_ms);

    span.in_scope(|| {
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
    });
}
