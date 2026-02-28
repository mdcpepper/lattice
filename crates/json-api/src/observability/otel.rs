//! OpenTelemetry tracer provider setup.

use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
};

use crate::config::ServerConfig;

use super::ObservabilityError;

pub(super) fn build_tracer_provider(
    config: &ServerConfig,
) -> Result<SdkTracerProvider, ObservabilityError> {
    let resource = Resource::builder_empty()
        .with_service_name(config.otel_service_name.clone())
        .with_attributes([
            KeyValue::new("service.version", config.otel_service_version.clone()),
            KeyValue::new(
                "deployment.environment.name",
                config.otel_deployment_environment.clone(),
            ),
        ])
        .build();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.otel_exporter_otlp_endpoint.clone())
        .with_timeout(Duration::from_secs(
            config.otel_exporter_otlp_timeout_seconds,
        ))
        .build()?;

    Ok(SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            clamp_sample_ratio(config.otel_trace_sample_ratio),
        ))))
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build())
}

fn clamp_sample_ratio(sample_ratio: f64) -> f64 {
    sample_ratio.clamp(0.0, 1.0)
}
