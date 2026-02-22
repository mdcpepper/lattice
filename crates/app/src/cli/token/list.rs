use clap::Args;
use lattice_app::{auth::PgAuthRepository, database};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(crate) struct ListTokensArgs {
    /// PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Tenant UUID whose tokens should be listed
    #[arg(long)]
    tenant_uuid: Uuid,
}

pub(crate) async fn run(args: ListTokensArgs) -> Result<(), String> {
    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let repository = PgAuthRepository::new(pool);

    let tokens = repository
        .list_api_tokens_by_tenant(args.tenant_uuid.into())
        .await
        .map_err(|error| format!("failed to list tokens: {error}"))?;

    if tokens.is_empty() {
        println!("no tokens found for tenant {}", args.tenant_uuid);
        return Ok(());
    }

    for token in tokens {
        println!("token_uuid: {}", token.uuid);
        println!("tenant_uuid: {}", token.tenant_uuid);
        println!("token_version: {}", token.version.as_i16());
        println!("created_at: {}", token.created_at);
        println!(
            "last_used_at: {}",
            token
                .last_used_at
                .map_or_else(|| "never".to_string(), |value| value.to_string())
        );
        println!(
            "expires_at: {}",
            token
                .expires_at
                .map_or_else(|| "none".to_string(), |value| value.to_string())
        );
        println!(
            "revoked_at: {}",
            token
                .revoked_at
                .map_or_else(|| "active".to_string(), |value| value.to_string())
        );
        println!();
    }

    Ok(())
}
