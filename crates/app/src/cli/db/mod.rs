use clap::{Args, Subcommand};

mod ensure_app_role;

#[derive(Debug, Args)]
pub(crate) struct DbCommand {
    #[command(subcommand)]
    command: DbSubcommand,
}

#[derive(Debug, Subcommand)]
enum DbSubcommand {
    EnsureAppRole(ensure_app_role::EnsureAppRoleArgs),
}

pub(crate) async fn run(command: DbCommand) -> Result<(), String> {
    match command.command {
        DbSubcommand::EnsureAppRole(args) => ensure_app_role::run(args).await,
    }
}
