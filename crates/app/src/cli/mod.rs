use clap::{Parser, Subcommand};

mod db;
mod tenant;
mod token;

#[derive(Debug, Parser)]
#[command(name = "lattice-app", about = "Lattice CLI", long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Tenant(tenant::TenantCommand),
    Token(token::TokenCommand),
    Db(db::DbCommand),
}

impl Cli {
    pub(crate) async fn run(self) -> Result<(), String> {
        match self.command {
            Commands::Tenant(command) => tenant::run(command).await,
            Commands::Token(command) => token::run(command).await,
            Commands::Db(command) => db::run(command).await,
        }
    }
}
