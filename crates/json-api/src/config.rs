//! Server configuration module

use clap::Parser;

/// Log output format.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum LogFormat {
    /// Compact, human-readable logs.
    Compact,

    /// Structured JSON logs.
    Json,
}

/// Lattice JSON API Server configuration
#[derive(Debug, Parser)]
#[command(name = "lattice-json", about = "Lattice JSON API Server", long_about = None)]
pub struct ServerConfig {
    /// Server host address
    #[arg(short = 'H', long, env = "SERVER_HOST", default_value = "0.0.0.0")]
    pub host: String,

    /// Server port
    #[arg(short, long, env = "SERVER_PORT", default_value = "8698")]
    pub port: u16,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    /// Log format (compact, json)
    #[arg(long, env = "LOG_FORMAT", value_enum, default_value_t = LogFormat::Compact)]
    pub log_format: LogFormat,

    /// Enable OpenTelemetry tracing export.
    #[arg(long, env = "OTEL_ENABLED", default_value_t = true)]
    pub otel_enabled: bool,

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

    /// Threshold for slow request warnings.
    #[arg(long, env = "SLOW_REQUEST_THRESHOLD_MS", default_value_t = 1_000_u64)]
    pub slow_request_threshold_ms: u64,

    /// `PostgreSQL` connection string
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,

    /// `OpenBao` server address
    #[arg(long, env = "OPENBAO_ADDR")]
    pub openbao_addr: String,

    /// `OpenBao` authentication token
    #[arg(long, env = "OPENBAO_TOKEN", hide_env_values = true)]
    pub openbao_token: String,

    /// `OpenBao` Transit key name
    #[arg(long, env = "OPENBAO_TRANSIT_KEY")]
    pub openbao_transit_key: String,
}

impl ServerConfig {
    /// Load configuration from environment and CLI arguments
    ///
    /// # Errors
    ///
    /// Returns an error if configuration cannot be parsed
    pub fn load() -> Result<Self, clap::Error> {
        // Load .env file if present (ignore if missing)
        _ = dotenvy::dotenv();

        Self::try_parse()
    }

    /// Get the socket address for binding
    #[must_use]
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
