//! Utils

use clap::Parser;
use slotmap::SlotMap;

use crate::{
    promotions::{PromotionSlotKey, mix_and_match::MixAndMatchSlot},
    tags::string::StringTagCollection,
};

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

/// Create a new promotion slot with the given tags, minimum and maximum values.
pub fn slot(
    keys: &mut SlotMap<PromotionSlotKey, ()>,
    tags: StringTagCollection,
    min: usize,
    max: Option<usize>,
) -> MixAndMatchSlot {
    MixAndMatchSlot::new(keys.insert(()), tags, min, max)
}
