//! Database connection management

use std::process;

use sqlx::PgPool;
use tracing::error;

/// Connect to PostgreSQL, exiting the process on failure.
pub(crate) async fn connect(database_url: &str) -> PgPool {
    PgPool::connect(database_url).await.unwrap_or_else(|error| {
        error!("failed to connect to postgres: {error}");
        process::exit(1);
    })
}
