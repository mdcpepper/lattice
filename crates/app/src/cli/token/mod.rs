use clap::{Args, Subcommand};

mod create;
mod list;
mod revoke;

#[derive(Debug, Args)]
pub(crate) struct TokenCommand {
    #[command(subcommand)]
    command: TokenSubcommand,
}

#[derive(Debug, Subcommand)]
enum TokenSubcommand {
    Create(create::CreateTokenArgs),
    List(list::ListTokensArgs),
    Revoke(revoke::RevokeTokenArgs),
}

pub(crate) async fn run(command: TokenCommand) -> Result<(), String> {
    match command.command {
        TokenSubcommand::Create(args) => create::run(args).await,
        TokenSubcommand::List(args) => list::run(args).await,
        TokenSubcommand::Revoke(args) => revoke::run(args).await,
    }
}
