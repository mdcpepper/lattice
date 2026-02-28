//! Prometheus metrics collection and exposition endpoint.

use std::sync::OnceLock;

use prometheus::{
    Encoder, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder,
};
use salvo::{
    Request, Response, handler,
    http::{
        StatusCode,
        header::{CONTENT_TYPE, HeaderValue},
    },
};
use tracing::error;

#[derive(Debug)]
struct HttpMetrics {
    registry: Registry,
    requests_total: IntCounterVec,
    request_duration_seconds: HistogramVec,
    requests_in_flight: IntGauge,
}

static HTTP_METRICS: OnceLock<Option<HttpMetrics>> = OnceLock::new();

#[derive(Debug)]
pub(super) struct InFlightRequestGuard {
    tracked: bool,
}

impl InFlightRequestGuard {
    pub(super) fn track() -> Self {
        if let Some(metrics) = metrics() {
            metrics.requests_in_flight.inc();
            return Self { tracked: true };
        }

        Self { tracked: false }
    }
}

impl Drop for InFlightRequestGuard {
    fn drop(&mut self) {
        if self.tracked
            && let Some(metrics) = metrics()
        {
            metrics.requests_in_flight.dec();
        }
    }
}

pub(super) fn observe_request(method: &str, route: &str, status_code: u16, duration_seconds: f64) {
    let Some(metrics) = metrics() else {
        return;
    };

    let status_class = status_class(status_code);
    let status_code = status_code.to_string();

    metrics
        .requests_total
        .with_label_values(&[method, route, status_class, status_code.as_str()])
        .inc();

    metrics
        .request_duration_seconds
        .with_label_values(&[method, route])
        .observe(duration_seconds);
}

#[handler]
pub(crate) async fn metrics_handler(_req: &mut Request, res: &mut Response) {
    let Some(metrics) = metrics() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        return;
    };

    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();

    let mut encoded = Vec::new();

    if let Err(source) = encoder.encode(&metric_families, &mut encoded) {
        error!("failed to encode metrics response: {source}");
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);

        return;
    }

    let content_type = match HeaderValue::from_str(encoder.format_type()) {
        Ok(value) => value,
        Err(source) => {
            error!("failed to encode metrics content type header: {source}");
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);

            return;
        }
    };

    res.headers_mut().insert(CONTENT_TYPE, content_type);
    res.render(String::from_utf8_lossy(&encoded).into_owned());
}

fn metrics() -> Option<&'static HttpMetrics> {
    HTTP_METRICS.get_or_init(build_metrics).as_ref()
}

fn build_metrics() -> Option<HttpMetrics> {
    let registry = Registry::new();

    let requests_total = match IntCounterVec::new(
        Opts::new(
            "lattice_json_http_requests_total",
            "Total HTTP requests partitioned by method, route, status class, and status code.",
        ),
        &["method", "route", "status_class", "status_code"],
    ) {
        Ok(metric) => metric,
        Err(source) => {
            error!("failed to create requests_total metric: {source}");
            return None;
        }
    };

    let request_duration_seconds = match HistogramVec::new(
        HistogramOpts::new(
            "lattice_json_http_request_duration_seconds",
            "HTTP request duration in seconds partitioned by method and route.",
        )
        .buckets(vec![
            0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]),
        &["method", "route"],
    ) {
        Ok(metric) => metric,
        Err(source) => {
            error!("failed to create request_duration metric: {source}");
            return None;
        }
    };

    let requests_in_flight = match IntGauge::with_opts(Opts::new(
        "lattice_json_http_requests_in_flight",
        "Current number of in-flight HTTP requests.",
    )) {
        Ok(metric) => metric,
        Err(source) => {
            error!("failed to create in-flight gauge metric: {source}");
            return None;
        }
    };

    if let Err(source) = registry.register(Box::new(requests_total.clone())) {
        error!("failed to register requests_total metric: {source}");
        return None;
    }

    if let Err(source) = registry.register(Box::new(request_duration_seconds.clone())) {
        error!("failed to register request_duration metric: {source}");
        return None;
    }

    if let Err(source) = registry.register(Box::new(requests_in_flight.clone())) {
        error!("failed to register in-flight gauge metric: {source}");
        return None;
    }

    Some(HttpMetrics {
        registry,
        requests_total,
        request_duration_seconds,
        requests_in_flight,
    })
}

fn status_class(status_code: u16) -> &'static str {
    match status_code {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use salvo::{
        Router, Service,
        test::{ResponseExt, TestClient},
    };

    use super::{metrics_handler, observe_request};

    #[tokio::test]
    async fn metrics_endpoint_exposes_http_metrics() {
        observe_request("GET", "/products", 200, 0.042);
        observe_request("GET", "/products", 500, 0.123);

        let service =
            Service::new(Router::new().push(Router::with_path("metrics").get(metrics_handler)));

        let response_result = TestClient::get("http://example.com/metrics")
            .send(&service)
            .await
            .take_string()
            .await;

        let response: String = response_result.unwrap_or_default();

        assert!(
            response.contains("lattice_json_http_requests_total"),
            "expected requests_total metric in response"
        );
        assert!(
            response.contains("lattice_json_http_request_duration_seconds"),
            "expected request_duration metric in response"
        );
        assert!(
            response.contains("lattice_json_http_requests_in_flight"),
            "expected in-flight metric in response"
        );
    }
}
