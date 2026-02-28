//! Server Config

use clap::Args;

/// Server runtime network settings.
#[derive(Debug, Args)]
pub struct ServerRuntimeConfig {
    /// Server host address
    #[arg(short = 'H', long, env = "SERVER_HOST", default_value = "0.0.0.0")]
    pub host: String,

    /// Server port
    #[arg(short, long, env = "SERVER_PORT", default_value = "8698")]
    pub port: u16,
}

impl ServerRuntimeConfig {
    /// Get the socket address for binding.
    #[must_use]
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
