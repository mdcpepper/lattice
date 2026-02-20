//! Database connection management

use sqlx::PgPool;

/// Connect to `PostgreSQL`.
///
/// # Errors
///
/// Returns an error if the connection cannot be established.
pub(crate) async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPool::connect(database_url).await
}
