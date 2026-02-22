use clap::Args;
use lattice_app::{
    database,
    tenants::{PgTenantsService, TenantsService, models::NewTenant},
};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(crate) struct CreateTenantArgs {
    /// Tenant display name
    #[arg(long)]
    name: String,

    /// PostgreSQL connection string
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    /// Optional tenant UUID; generated when omitted
    #[arg(long)]
    tenant_uuid: Option<Uuid>,
}

pub(crate) async fn run(args: CreateTenantArgs) -> Result<(), String> {
    let pool = database::connect(&args.database_url)
        .await
        .map_err(|error| format!("failed to connect to database: {error}"))?;

    let service = PgTenantsService::new(pool);
    let tenant_uuid = args.tenant_uuid.unwrap_or_else(Uuid::now_v7);

    let tenant = service
        .create_tenant(NewTenant {
            uuid: tenant_uuid,
            name: args.name,
        })
        .await
        .map_err(|error| format!("failed to create tenant: {error}"))?;

    println!("tenant_uuid: {}", tenant.uuid);
    println!("tenant_name: {}", tenant.name);

    Ok(())
}
