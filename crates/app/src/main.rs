//! Lattice Application CLI

use std::process;

use clap::Parser;

mod cli;

#[tokio::main]
pub async fn main() {
    let _env = dotenvy::dotenv();

    let cli = cli::Cli::parse();

    if let Err(error) = cli.run().await {
        eprintln!("{error}");
        process::exit(1);
    }
}
