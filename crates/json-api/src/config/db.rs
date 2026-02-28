//! Database Config

use clap::Args;

/// Database settings.
#[derive(Debug, Args)]
pub struct DatabaseConfig {
    /// `PostgreSQL` connection string
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,
}
