//! Observability setup and request tracing middleware.

use thiserror::Error;

mod init;
mod otel;
mod request;
mod settings;

pub(crate) use init::Observability;
pub(crate) use request::request_logging;

/// Errors raised while initializing observability.
#[derive(Debug, Error)]
pub(crate) enum ObservabilityError {
    /// Failed to build OTLP exporter.
    #[error("failed to build OTLP exporter: {0}")]
    OtlpExporter(#[from] opentelemetry_otlp::ExporterBuildError),

    /// Failed to initialize tracing subscriber.
    #[error("failed to initialize tracing subscriber: {0}")]
    TracingSubscriber(#[from] tracing_subscriber::util::TryInitError),
}
