use clap::Args;
use lattice_app::{auth::PgAuthRepository, database};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(crate) struct RevokeTokenArgs {
    /// PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Token UUID to revoke
    #[arg(long)]
    token_uuid: Uuid,
}

pub(crate) async fn run(args: RevokeTokenArgs) -> Result<(), String> {
    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let repository = PgAuthRepository::new(pool);

    let revoked = repository
        .revoke_api_token(args.token_uuid)
        .await
        .map(|record| record.is_some())
        .map_err(|error| format!("failed to revoke token: {error}"))?;

    if revoked {
        println!("revoked token {}", args.token_uuid);
    } else {
        println!("token {} was not active", args.token_uuid);
    }

    Ok(())
}
