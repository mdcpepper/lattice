//! Lattice Application CLI

use std::process;

use clap::{Args, Parser, Subcommand};
use lattice_app::{
    database,
    tenants::{PgTenantsService, TenantsService, models::NewTenant},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "lattice-app", about = "Lattice CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Tenant(TenantCommand),
}

#[derive(Debug, Args)]
struct TenantCommand {
    #[command(subcommand)]
    command: TenantSubcommand,
}

#[derive(Debug, Subcommand)]
enum TenantSubcommand {
    Create(CreateTenantArgs),
}

#[derive(Debug, Args)]
struct CreateTenantArgs {
    /// Tenant display name
    #[arg(long)]
    name: String,

    /// PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Optional tenant UUID; generated when omitted
    #[arg(long)]
    tenant_uuid: Option<Uuid>,

    /// Optional raw API token; generated when omitted
    #[arg(long)]
    token: Option<String>,
}

#[tokio::main]
pub async fn main() {
    let _env = dotenvy::dotenv();

    let cli = Cli::parse();

    if let Err(error) = run(cli).await {
        eprintln!("{error}");
        process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Tenant(TenantCommand {
            command: TenantSubcommand::Create(args),
        }) => create_tenant(args).await,
    }
}

async fn create_tenant(args: CreateTenantArgs) -> Result<(), String> {
    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let service = PgTenantsService::new(pool);
    let tenant_uuid = args.tenant_uuid.unwrap_or_else(Uuid::now_v7);
    let raw_token = args.token.unwrap_or_else(generate_token);

    if raw_token.trim().is_empty() {
        return Err("token cannot be empty".to_string());
    }

    let tenant = service
        .create_tenant(NewTenant {
            uuid: tenant_uuid,
            name: args.name,
            token_uuid: Uuid::now_v7(),
            token_hash: hash_token(&raw_token),
        })
        .await
        .map_err(|error| format!("failed to create tenant: {error}"))?;

    println!("tenant_uuid: {}", tenant.uuid);
    println!("tenant_name: {}", tenant.name);
    println!("api_token: {raw_token}");
    println!("store this token now; it is only shown once");

    Ok(())
}

fn generate_token() -> String {
    format!("lt_{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple())
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
