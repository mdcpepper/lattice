use clap::{Args, Subcommand};

mod create;

#[derive(Debug, Args)]
pub(crate) struct TenantCommand {
    #[command(subcommand)]
    command: TenantSubcommand,
}

#[derive(Debug, Subcommand)]
enum TenantSubcommand {
    Create(create::CreateTenantArgs),
}

pub(crate) async fn run(command: TenantCommand) -> Result<(), String> {
    match command.command {
        TenantSubcommand::Create(args) => create::run(args).await,
    }
}
