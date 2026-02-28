//! Observability & Logging Config

use clap::Args;

/// Log output format.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum LogFormat {
    /// Compact, human-readable logs.
    Compact,

    /// Structured JSON logs.
    Json,
}

/// Logging settings.
#[derive(Debug, Args)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    /// Log format (compact, json)
    #[arg(long, env = "LOG_FORMAT", value_enum, default_value_t = LogFormat::Compact)]
    pub log_format: LogFormat,
}

/// Observability settings.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent boolean feature toggles from CLI/env."
)]
pub struct ObservabilityConfig {
    /// Enable OpenTelemetry tracing export.
    #[arg(long, env = "OTEL_ENABLED", default_value_t = true)]
    pub otel_enabled: bool,

    /// Enable traceparent extraction from incoming request headers.
    #[arg(long, env = "OTEL_PARENT_PROPAGATION_ENABLED", default_value_t = false)]
    pub otel_parent_propagation_enabled: bool,

    /// OTLP gRPC endpoint for trace export.
    #[arg(
        long,
        env = "OTEL_EXPORTER_OTLP_ENDPOINT",
        default_value = "http://localhost:4317"
    )]
    pub otel_exporter_otlp_endpoint: String,

    /// OTLP exporter timeout in seconds.
    #[arg(
        long,
        env = "OTEL_EXPORTER_OTLP_TIMEOUT_SECONDS",
        default_value_t = 3u64
    )]
    pub otel_exporter_otlp_timeout_seconds: u64,

    /// OpenTelemetry service name.
    #[arg(long, env = "OTEL_SERVICE_NAME", default_value = "lattice-json")]
    pub otel_service_name: String,

    /// OpenTelemetry service version.
    #[arg(
        long,
        env = "OTEL_SERVICE_VERSION",
        default_value = env!("CARGO_PKG_VERSION")
    )]
    pub otel_service_version: String,

    /// OpenTelemetry deployment environment.
    #[arg(
        long,
        env = "OTEL_DEPLOYMENT_ENVIRONMENT",
        default_value = "development"
    )]
    pub otel_deployment_environment: String,

    /// Trace sampling ratio in range [0.0, 1.0].
    #[arg(long, env = "OTEL_TRACE_SAMPLE_RATIO", default_value_t = 1.0_f64)]
    pub otel_trace_sample_ratio: f64,

    /// Enable Pyroscope CPU profiling export.
    #[arg(long, env = "PYROSCOPE_ENABLED", default_value_t = false)]
    pub pyroscope_enabled: bool,

    /// Pyroscope server address.
    #[arg(
        long,
        env = "PYROSCOPE_SERVER_ADDRESS",
        default_value = "http://localhost:4040"
    )]
    pub pyroscope_server_address: String,

    /// Pyroscope sample rate in Hertz.
    #[arg(long, env = "PYROSCOPE_SAMPLE_RATE", default_value_t = 100_u32)]
    pub pyroscope_sample_rate: u32,

    /// Enable per-request Pyroscope tags (http.method/http.route).
    #[arg(long, env = "PYROSCOPE_REQUEST_TAGS_ENABLED", default_value_t = false)]
    pub pyroscope_request_tags_enabled: bool,

    /// Threshold for slow request warnings.
    #[arg(long, env = "SLOW_REQUEST_THRESHOLD_MS", default_value_t = 1_000_u64)]
    pub slow_request_threshold_ms: u64,
}
