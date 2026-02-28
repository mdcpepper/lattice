//! Logging subscriber initialisation.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::{
    EnvFilter, Registry,
    layer::{Layer, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::config::{ServerConfig, observability::LogFormat};

use super::ObservabilityError;

pub(super) fn init_subscriber(
    config: &ServerConfig,
    tracer_provider: Option<&SdkTracerProvider>,
) -> Result<(), ObservabilityError> {
    match config.logging.log_format {
        LogFormat::Compact => init_with_layer(
            config,
            tracing_subscriber::fmt::layer()
                .compact()
                .with_target(true)
                .with_file(true)
                .with_line_number(true),
            tracer_provider,
        ),
        LogFormat::Json => init_with_layer(
            config,
            tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_target(true),
            tracer_provider,
        ),
    }
}

fn build_env_filter(config: &ServerConfig) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{},h2=warn,hyper=warn,tower=warn,tonic=warn,opentelemetry=warn",
            config.logging.log_level
        ))
    })
}

fn init_with_layer<L>(
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
        let tracer = tracer_provider.tracer(config.observability.otel_service_name.clone());

        subscriber
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()?;
    } else {
        subscriber.try_init()?;
    }

    Ok(())
}
