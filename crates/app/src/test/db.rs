//! Database test utilities and shared infrastructure

use once_cell::sync::Lazy;
use sqlx::{Connection, PgConnection, PgPool, Postgres, Transaction};
use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres as PostgresImage;
use tokio::sync::{OnceCell, mpsc};

/// Validates a database name to prevent SQL injection
///
/// Database names must:
/// - Be 1-63 characters long
/// - Start with a letter or underscore
/// - Contain only letters, digits, underscores, and dollar signs
/// - Not be a PostgreSQL reserved word
fn validate_database_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 63 {
        return Err("Database name must be 1-63 characters long".to_string());
    }

    // Must start with letter or underscore
    if !name.chars().next().unwrap().is_ascii_alphabetic() && !name.starts_with('_') {
        return Err("Database name must start with a letter or underscore".to_string());
    }

    // Must contain only valid characters
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    {
        return Err(
            "Database name can only contain letters, digits, underscores, and dollar signs"
                .to_string(),
        );
    }

    // Check against some common PostgreSQL reserved words that could be problematic
    let reserved_words = [
        "user", "table", "select", "insert", "update", "delete", "drop", "create", "alter",
        "index", "database", "schema", "role", "grant", "revoke",
    ];

    if reserved_words
        .iter()
        .any(|&word| name.eq_ignore_ascii_case(word))
    {
        return Err(format!("Database name '{name}' is a reserved word"));
    }

    Ok(())
}

/// Shared PostgreSQL container initialization
async fn init_postgres_container() -> ContainerAsync<PostgresImage> {
    PostgresImage::default()
        .with_user("lattice_test")
        .with_password("lattice_test_password")
        .with_db_name("lattice_test")
        .with_env_var("POSTGRES_INITDB_ARGS", "--auth-host=trust")
        .start()
        .await
        .expect("Failed to start PostgreSQL container")
}

/// Shared PostgreSQL container that starts once and is reused across all tests
static POSTGRES_CONTAINER: Lazy<OnceCell<ContainerAsync<PostgresImage>>> = Lazy::new(OnceCell::new);

/// Cleanup channel for database cleanup requests
static CLEANUP_SENDER: Lazy<OnceCell<mpsc::UnboundedSender<String>>> = Lazy::new(OnceCell::new);

/// Initialize the cleanup background task
async fn init_cleanup_task() -> mpsc::UnboundedSender<String> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<String>();

    // Spawn background task to handle cleanup requests
    tokio::spawn(async move {
        while let Some(db_name) = receiver.recv().await {
            // Perform async cleanup
            if let Err(err) = cleanup_database(&db_name).await {
                eprintln!("Failed to cleanup database '{db_name}': {err}");
            }
        }
    });

    sender
}

/// Drop a test database by name.
async fn cleanup_database(db_name: &str) -> Result<(), sqlx::Error> {
    if let Some(container) = POSTGRES_CONTAINER.get()
        && let Ok(port) = container.get_host_port_ipv4(5432).await
    {
        let host = std::env::var("TESTCONTAINERS_HOST_OVERRIDE")
            .unwrap_or_else(|_| "localhost".to_string());
        let base_url =
            format!("postgresql://lattice_test:lattice_test_password@{host}:{port}/postgres");

        if let Ok(mut conn) = PgConnection::connect(&base_url).await {
            // Validate database name before dropping
            if validate_database_name(db_name).is_ok() {
                let drop_query = format!("DROP DATABASE IF EXISTS \"{db_name}\"");
                let _ = sqlx::query(&drop_query).execute(&mut conn).await;
            }
            let _ = conn.close().await;
        }
    }

    Ok(())
}

/// Test database configuration
///
/// Each `TestDb` instance creates a uniquely named database within a shared PostgreSQL container.
/// The database is automatically dropped when the `TestDb` instance goes out of scope.
///
/// ## Isolation model
///
/// Isolation is **database-level**: every test gets its own fresh database with migrations
/// applied. Service methods commit their own transactions normally, so there is no
/// auto-rollback mechanism. Tests do not need to do anything special to get clean state —
/// it comes for free from the per-test database.
///
/// `cleanup().await` drops the database immediately rather than waiting for `Drop`. It is
/// purely an optimisation for long test suites and is never required for correctness.
#[derive(Debug, Clone)]
pub struct TestDb {
    /// PostgreSQL connection pool
    pub pool: PgPool,

    /// PostgreSQL database name
    pub name: String,

    /// The URL `pool` was connected with. Stored so callers (e.g. `TestContext`) can
    /// derive alternate connection strings by substituting different credentials.
    pub(super) superuser_url: String,
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Send cleanup request to background task
        if let Some(sender) = CLEANUP_SENDER.get() {
            let _ = sender.send(self.name.clone());
        }
    }
}

impl TestDb {
    /// Create an isolated test database with a unique generated name.
    #[allow(dead_code)]
    pub async fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let thread_id = std::thread::current().id();

        let name =
            format!("lattice_repo_test_{nanos}_{thread_id:?}").replace([':', ' ', '(', ')'], "");

        Self::new_with_db_name(&name).await
    }

    /// Create an isolated test database with the given name.
    #[allow(dead_code)]
    pub async fn new_with_db_name(db_name: &str) -> Self {
        let _cleanup_sender = CLEANUP_SENDER.get_or_init(init_cleanup_task).await;

        if let Err(error) = validate_database_name(db_name) {
            panic!("Invalid database name '{db_name}': {error}");
        }

        let container = POSTGRES_CONTAINER
            .get_or_init(init_postgres_container)
            .await;

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("Failed to get container port");

        let host = std::env::var("TESTCONTAINERS_HOST_OVERRIDE")
            .unwrap_or_else(|_| "localhost".to_string());

        let base_url =
            format!("postgresql://lattice_test:lattice_test_password@{host}:{port}/postgres");

        let mut conn = PgConnection::connect(&base_url)
            .await
            .expect("Failed to connect to postgres database");

        // Create the isolated test database
        let create_db_query = format!("CREATE DATABASE \"{db_name}\"");

        sqlx::query(&create_db_query)
            .execute(&mut conn)
            .await
            .expect("Failed to create test database");

        conn.close()
            .await
            .expect("Failed to close admin connection");

        // Now connect to the isolated database
        let database_url =
            format!("postgresql://lattice_test:lattice_test_password@{host}:{port}/{db_name}");

        println!("Connecting to database: {database_url}");

        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to create pool for database");

        let instance = Self {
            pool,
            name: db_name.to_string(),
            superuser_url: database_url,
        };

        // Run migrations on this database
        println!("Running migrations on database: {db_name}");

        sqlx::migrate!("../../migrations")
            .run(&instance.pool)
            .await
            .expect("Failed to run migrations on database");

        instance
    }

    /// Drop the isolated database immediately rather than waiting until `TestDb` is dropped.
    ///
    /// This is purely an optimisation — cleanup also happens automatically on drop, so
    /// calling this is never required for test correctness.
    pub async fn cleanup(&self) {
        if let Some(sender) = CLEANUP_SENDER.get() {
            let _ = sender.send(self.name.clone());
        }
    }

    /// Begin a transaction against the test database.
    ///
    /// The transaction rolls back automatically when dropped, which can be useful for
    /// low-level repository tests that want to inspect intermediate state without
    /// committing. Service-level tests should use [`TestContext`] instead, which relies
    /// on per-test database isolation rather than rollback.
    pub async fn begin_test_transaction(&self) -> Transaction<'_, Postgres> {
        self.pool
            .begin()
            .await
            .expect("Failed to start test transaction")
    }

    /// Returns the connection pool for this test database.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_database_name_success() {
        assert!(validate_database_name("valid_name").is_ok());
        assert!(validate_database_name("_underscore_start").is_ok());
        assert!(validate_database_name("test123").is_ok());
        assert!(validate_database_name("my_test_db").is_ok());
        assert!(validate_database_name("db$with$dollar").is_ok());
    }

    #[test]
    fn test_validate_database_name_empty() {
        assert!(validate_database_name("").is_err());
    }

    #[test]
    fn test_validate_database_name_too_long() {
        let long_name = "a".repeat(64);
        assert!(validate_database_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_database_name_invalid_start() {
        assert!(validate_database_name("123invalid").is_err());
        assert!(validate_database_name("-invalid").is_err());
        assert!(validate_database_name("$invalid").is_err());
    }

    #[test]
    fn test_validate_database_name_invalid_characters() {
        assert!(validate_database_name("invalid-hyphen").is_err());
        assert!(validate_database_name("invalid.dot").is_err());
        assert!(validate_database_name("invalid space").is_err());
        assert!(validate_database_name("invalid@symbol").is_err());
    }

    #[test]
    fn test_validate_database_name_reserved_words() {
        assert!(validate_database_name("user").is_err());
        assert!(validate_database_name("USER").is_err());
        assert!(validate_database_name("table").is_err());
        assert!(validate_database_name("select").is_err());
        assert!(validate_database_name("database").is_err());
    }

    #[tokio::test]
    async fn test_container_startup() {
        let test_db = TestDb::new().await;

        // Verify we can connect and run a simple query
        let result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(test_db.pool())
            .await
            .expect("Failed to execute test query");

        assert_eq!(result, 1);
    }

    #[tokio::test]
    async fn test_transaction_isolation() {
        let test_db = TestDb::new().await;

        // Create test table in transaction 1
        let mut tx1 = test_db.begin_test_transaction().await;

        sqlx::query("CREATE TABLE test_isolation (id INTEGER)")
            .execute(&mut *tx1)
            .await
            .expect("Failed to create table in tx1");

        // Transaction 2 should not see the table
        let mut tx2 = test_db.begin_test_transaction().await;

        let result = sqlx::query("SELECT COUNT(*) FROM test_isolation")
            .fetch_one(&mut *tx2)
            .await;

        // Should fail because table doesn't exist in tx2
        assert!(result.is_err());

        // Both transactions roll back automatically when dropped
    }

    #[tokio::test]
    async fn test_cleanup_method_execution() {
        let test_db = TestDb::new().await;

        // Verify the database exists and is functional
        let result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(test_db.pool())
            .await
            .expect("Failed to execute test query");
        assert_eq!(result, 1);

        test_db.cleanup().await;
    }
}
