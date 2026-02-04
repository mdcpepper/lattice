//! Direct Discount Promotion Fixtures

use decimal_percentage::Percentage;
use rusty_money::Money;
use serde::Deserialize;

use crate::{
    fixtures::{FixtureError, products::parse_price},
    promotions::direct_discount::DirectDiscount,
};

/// Direct Discount configuration from YAML fixtures
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DirectDiscountFixtureConfig {
    /// Percentage discount (value between 0.0 and 1.0)
    Percentage {
        /// Discount percentage as decimal (e.g., 0.15 for 15%)
        value: f64,
    },

    /// Fixed price override (e.g., "2.50 GBP")
    AmountOverride {
        /// Price string (e.g., "2.50 GBP")
        value: String,
    },

    /// Fixed amount discount off (e.g., "0.75 GBP")
    AmountDiscountOff {
        /// Discount amount string (e.g., "0.75 GBP")
        value: String,
    },
}

impl TryFrom<DirectDiscountFixtureConfig> for DirectDiscount<'_> {
    type Error = FixtureError;

    fn try_from(config: DirectDiscountFixtureConfig) -> Result<Self, Self::Error> {
        match config {
            DirectDiscountFixtureConfig::Percentage { value } => {
                Ok(DirectDiscount::Percentage(Percentage::from(value)))
            }
            DirectDiscountFixtureConfig::AmountOverride { value } => {
                let (minor_units, currency) = parse_price(&value)?;

                Ok(DirectDiscount::AmountOverride(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            DirectDiscountFixtureConfig::AmountDiscountOff { value } => {
                let (minor_units, currency) = parse_price(&value)?;

                Ok(DirectDiscount::AmountOff(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
        }
    }
}
