//! Server configuration module

use clap::Parser;

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
