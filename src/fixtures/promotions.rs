//! Promotion Fixtures

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::{
    fixtures::{FixtureError, promotions::direct_discount::DirectDiscountFixtureConfig},
    promotions::{
        Promotion, PromotionKey, PromotionMeta,
        direct_discount::{DirectDiscount, DirectDiscountPromotion},
    },
    tags::string::StringTagCollection,
};

mod direct_discount;

/// Wrapper for promotions in YAML
#[derive(Debug, Deserialize)]
pub struct PromotionsFixture {
    /// Map of promotion key -> promotion fixture
    pub promotions: FxHashMap<String, PromotionFixture>,
}

/// Promotion fixture from YAML
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromotionFixture {
    /// Direct discount promotion
    DirectDiscount {
        /// Promotion name
        name: String,

        /// Promotion tags for targeting
        tags: Vec<String>,

        /// Discount configuration
        discount: DirectDiscountFixtureConfig,
    },
}

impl PromotionFixture {
    /// Convert to `PromotionMeta` and `Promotion`
    ///
    /// # Errors
    ///
    /// Returns an error if the discount configuration is invalid.
    pub fn try_into_promotion(
        self,
        key: PromotionKey,
    ) -> Result<(PromotionMeta, Promotion<'static>), FixtureError> {
        match self {
            PromotionFixture::DirectDiscount {
                name,
                tags,
                discount,
            } => {
                let meta = PromotionMeta { name: name.clone() };

                // Convert discount using TryFrom
                let config = DirectDiscount::try_from(discount)?;
                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
                let tags_collection = StringTagCollection::from_strs(&tag_refs);

                let direct_discount = DirectDiscountPromotion::new(key, tags_collection, config);
                let promotion = Promotion::DirectDiscount(direct_discount);

                Ok((meta, promotion))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use rusty_money::iso::GBP;

    use super::*;

    #[test]
    fn promotion_fixture_rejects_unknown_type() {
        let yaml = r"
type: unknown_promotion
name: Test
tags: []
discount:
  type: percentage
  value: 0.10
";
        let result: Result<PromotionFixture, _> = serde_norway::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn discount_fixture_parses_percentage() -> Result<(), FixtureError> {
        let fixture = DirectDiscountFixtureConfig::Percentage { value: 0.15 };

        let config = DirectDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            DirectDiscount::Percentage(percent) if percent == Percentage::from(0.15)
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_parses_amount_override() -> Result<(), FixtureError> {
        let fixture = DirectDiscountFixtureConfig::AmountOverride {
            value: "2.50 GBP".to_string(),
        };

        let config = DirectDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            DirectDiscount::AmountOverride(money) if money.to_minor_units() == 250
                && money.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_parses_amount_discount_off() -> Result<(), FixtureError> {
        let fixture = DirectDiscountFixtureConfig::AmountDiscountOff {
            value: "0.75 GBP".to_string(),
        };

        let config = DirectDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            DirectDiscount::AmountOff(money) if money.to_minor_units() == 75
                && money.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_rejects_unknown_discount_type() {
        let yaml = r"
type: mystery_discount
value: 0.10
";
        let result: Result<DirectDiscountFixtureConfig, _> = serde_norway::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn discount_fixture_rejects_string_for_percentage() {
        let yaml = r"
type: percentage
value: not a number
";
        let result: Result<DirectDiscountFixtureConfig, _> = serde_norway::from_str(yaml);
        assert!(result.is_err());
    }
}
