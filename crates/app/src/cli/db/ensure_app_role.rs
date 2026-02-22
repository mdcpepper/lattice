use clap::Args;
use lattice_app::database;
use sqlx::{query, query_scalar};

#[derive(Debug, Args)]
pub(crate) struct EnsureAppRoleArgs {
    /// Administrative PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Application runtime role name
    #[arg(long, default_value = "lattice_app")]
    role_name: String,

    /// Application role password
    #[arg(long, env = "APP_DB_PASSWORD", hide_env_values = true)]
    password: String,
}

pub(crate) async fn run(args: EnsureAppRoleArgs) -> Result<(), String> {
    // Reject empty role names early to avoid building invalid SQL.
    if args.role_name.trim().is_empty() {
        return Err("role_name cannot be empty".to_string());
    }

    // Require a non-empty password; this command is intended to rotate/set it.
    if args.password.trim().is_empty() {
        return Err("password cannot be empty".to_string());
    }

    // Connect with an administrative role; runtime app credentials are not enough
    // for CREATE/ALTER ROLE and privilege management.
    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start transaction: {error}"))?;

    // Quote dynamic values server-side before interpolation into SQL statements
    // that cannot be fully parameterized (e.g. role identifiers).
    let role_ident: String = query_scalar("SELECT quote_ident($1)")
        .bind(&args.role_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| format!("failed to quote role_name: {error}"))?;

    let password_lit: String = query_scalar("SELECT quote_literal($1)")
        .bind(&args.password)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| format!("failed to quote password: {error}"))?;

    let role_exists: bool =
        query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(&args.role_name)
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| format!("failed to check role existence: {error}"))?;

    // Force runtime-safe role flags so API connections cannot bypass RLS.
    let upsert_role_sql = if role_exists {
        format!(
            "ALTER ROLE {role_ident} LOGIN PASSWORD {password_lit} NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS"
        )
    } else {
        format!(
            "CREATE ROLE {role_ident} LOGIN PASSWORD {password_lit} NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS"
        )
    };

    query(&upsert_role_sql)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to create/update role: {error}"))?;

    let database_ident: String = query_scalar("SELECT quote_ident(current_database())")
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| format!("failed to resolve database name: {error}"))?;

    // Apply required privileges for existing objects and default privileges for
    // future objects created in the public schema.
    let grant_sql = [
        format!("GRANT CONNECT ON DATABASE {database_ident} TO {role_ident}"),
        format!("GRANT USAGE ON SCHEMA public TO {role_ident}"),
        format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role_ident}"
        ),
        format!("GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO {role_ident}"),
        format!(
            "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {role_ident}"
        ),
        format!(
            "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT, UPDATE ON SEQUENCES TO {role_ident}"
        ),
    ];

    for sql in grant_sql {
        query(&sql)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to apply grant/default privilege `{sql}`: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit changes: {error}"))?;

    println!("ensured app role: {}", args.role_name);
    println!("applied grants for current database and public schema");

    Ok(())
}
