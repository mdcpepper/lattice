//! Observability setup and request tracing middleware.

use thiserror::Error;

mod init;
mod logging;
mod metrics;
mod otel;
mod profiling;
mod request;
mod settings;

pub(crate) use init::Observability;
pub(crate) use metrics::metrics_handler;
pub(crate) use request::request_logging;

/// Errors raised while initialising observability.
#[derive(Debug, Error)]
pub(crate) enum ObservabilityError {
    /// Failed to build OTLP exporter.
    #[error("failed to build OTLP exporter: {0}")]
    OtlpExporter(#[from] opentelemetry_otlp::ExporterBuildError),

    /// Failed to initialise tracing subscriber.
    #[error("failed to initialise tracing subscriber: {0}")]
    TracingSubscriber(#[from] tracing_subscriber::util::TryInitError),

    /// Failed to initialise pyroscope profiling.
    #[error("failed to initialise pyroscope profiling: {0}")]
    Pyroscope(#[from] pyroscope::PyroscopeError),
}
