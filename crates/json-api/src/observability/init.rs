//! Tracing subscriber and telemetry lifecycle management.

use opentelemetry::global;
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing::error;

use crate::config::ServerConfig;

use super::{ObservabilityError, logging, otel, profiling, settings};

/// Runtime observability state.
pub(crate) struct Observability {
    tracer_provider: Option<SdkTracerProvider>,
    profiling: profiling::Profiling,
}

impl Observability {
    /// Initialise structured logging and optional OpenTelemetry export.
    pub(crate) fn init(config: &ServerConfig) -> Result<Self, ObservabilityError> {
        settings::apply_runtime_config(config);

        let tracer_provider = if config.observability.otel_enabled {
            global::set_text_map_propagator(TraceContextPropagator::new());

            Some(otel::build_tracer_provider(config)?)
        } else {
            None
        };

        logging::init_subscriber(config, tracer_provider.as_ref())?;

        let profiling = profiling::Profiling::init(config)?;

        Ok(Self {
            tracer_provider,
            profiling,
        })
    }

    /// Flush and shutdown telemetry pipelines.
    pub(crate) fn shutdown(self) {
        self.profiling.shutdown();

        if let Some(provider) = self.tracer_provider
            && let Err(source) = provider.shutdown()
        {
            error!("failed to shutdown tracer provider: {source}");
        }
    }
}
