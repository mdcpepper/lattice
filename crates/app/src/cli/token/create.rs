use clap::Args;
use jiff::Timestamp;
use lattice_app::{
    auth::{OpenBaoClient, OpenBaoConfig, PgAuthService},
    database,
};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(crate) struct CreateTokenArgs {
    /// PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Tenant UUID that should own the token
    #[arg(long)]
    tenant_uuid: Uuid,

    /// OpenBao server address
    #[arg(long, env = "OPENBAO_ADDR")]
    openbao_addr: String,

    /// OpenBao authentication token
    #[arg(long, env = "OPENBAO_TOKEN", hide_env_values = true)]
    openbao_token: String,

    /// OpenBao Transit key name
    #[arg(long, env = "OPENBAO_TRANSIT_KEY")]
    openbao_transit_key: String,

    /// Optional token expiration timestamp (RFC 3339)
    #[arg(long)]
    token_expires_at: Option<String>,
}

pub(crate) async fn run(args: CreateTokenArgs) -> Result<(), String> {
    let token_expires_at = parse_token_expires_at(args.token_expires_at.as_deref())?;

    if let Some(expires_at) = token_expires_at.as_ref()
        && *expires_at <= Timestamp::now()
    {
        return Err("token-expires-at must be in the future".to_string());
    }

    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let openbao = OpenBaoClient::new(OpenBaoConfig {
        addr: args.openbao_addr,
        token: args.openbao_token,
        transit_key: args.openbao_transit_key,
    });
    let service = PgAuthService::new(pool, openbao);

    let issued = service
        .issue_api_token(args.tenant_uuid, token_expires_at)
        .await
        .map_err(|error| format!("failed to create token: {error}"))?;

    println!("token_uuid: {}", issued.metadata.uuid);
    println!("tenant_uuid: {}", issued.metadata.tenant_uuid);
    println!("token_version: {}", issued.metadata.version.as_i16());
    println!("token_created_at: {}", issued.metadata.created_at);
    if let Some(expires_at) = issued.metadata.expires_at {
        println!("token_expires_at: {expires_at}");
    }
    println!("api_token: {}", issued.token);
    println!("store this token now; it is only shown once");

    Ok(())
}

fn parse_token_expires_at(raw: Option<&str>) -> Result<Option<Timestamp>, String> {
    raw.map(|value| {
        value
            .parse::<Timestamp>()
            .map_err(|error| format!("invalid token-expires-at timestamp: {error}"))
    })
    .transpose()
}
