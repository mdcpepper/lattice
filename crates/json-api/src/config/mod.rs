//! Server configuration module

use clap::Parser;

use crate::config::{
    auth::AuthConfig,
    db::DatabaseConfig,
    observability::{LoggingConfig, ObservabilityConfig},
    server::ServerRuntimeConfig,
};

pub(crate) mod auth;
pub(crate) mod db;
pub(crate) mod observability;
pub(crate) mod server;

/// Lattice JSON API Server configuration
#[derive(Debug, Parser)]
#[command(name = "lattice-json", about = "Lattice JSON API Server", long_about = None)]
pub struct ServerConfig {
    /// Server network settings.
    #[command(flatten)]
    pub server: ServerRuntimeConfig,

    /// Logging output settings.
    #[command(flatten)]
    pub logging: LoggingConfig,

    /// Observability (traces/metrics/profiles) settings.
    #[command(flatten)]
    pub observability: ObservabilityConfig,

    /// Application database settings.
    #[command(flatten)]
    pub database: DatabaseConfig,

    /// `OpenBao` authentication settings.
    #[command(flatten)]
    pub auth: AuthConfig,
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
        self.server.socket_addr()
    }
}
