//! Utils

use clap::Parser;

/// Arguments for the basket examples
#[derive(Debug, Parser)]
pub struct ExampleBasketArgs {
    /// Number of items to add to the basket
    #[clap(short, long)]
    pub n: Option<usize>,

    /// Fixture set to use for the basket & promotions
    #[clap(short, long, default_value = "complex")]
    pub fixture: String,

    /// Output file path
    #[clap(short, long)]
    pub out: Option<String>,
}
