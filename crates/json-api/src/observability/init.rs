//! Tracing subscriber and telemetry lifecycle management.

use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing::error;
use tracing_subscriber::{
    EnvFilter, Registry,
    layer::{Layer, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::config::{LogFormat, ServerConfig};

use super::{ObservabilityError, otel, settings};

/// Runtime observability state.
#[derive(Debug)]
pub(crate) struct Observability {
    tracer_provider: Option<SdkTracerProvider>,
}

impl Observability {
    /// Initialize structured logging and optional OpenTelemetry export.
    pub(crate) fn init(config: &ServerConfig) -> Result<Self, ObservabilityError> {
        settings::apply_runtime_config(config);

        let tracer_provider = if config.otel_enabled {
            global::set_text_map_propagator(TraceContextPropagator::new());
            Some(otel::build_tracer_provider(config)?)
        } else {
            None
        };

        match config.log_format {
            LogFormat::Compact => init_subscriber(
                config,
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_target(true)
                    .with_file(true)
                    .with_line_number(true),
                tracer_provider.as_ref(),
            )?,
            LogFormat::Json => init_subscriber(
                config,
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_target(true),
                tracer_provider.as_ref(),
            )?,
        }

        Ok(Self { tracer_provider })
    }

    /// Flush and shutdown telemetry pipelines.
    pub(crate) fn shutdown(self) {
        let Some(provider) = self.tracer_provider else {
            return;
        };

        if let Err(source) = provider.shutdown() {
            error!("failed to shutdown tracer provider: {source}");
        }
    }
}

fn build_env_filter(config: &ServerConfig) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{},h2=warn,hyper=warn,tower=warn,tonic=warn,opentelemetry=warn",
            config.log_level
        ))
    })
}

fn init_subscriber<L>(
    config: &ServerConfig,
    fmt_layer: L,
    tracer_provider: Option<&SdkTracerProvider>,
) -> Result<(), ObservabilityError>
where
    L: Layer<Registry> + Send + Sync + 'static,
{
    let subscriber = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(build_env_filter(config));

    if let Some(tracer_provider) = tracer_provider {
        let tracer = tracer_provider.tracer(config.otel_service_name.clone());
        subscriber
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()?;
    } else {
        subscriber.try_init()?;
    }

    Ok(())
}
