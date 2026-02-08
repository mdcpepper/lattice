//! Promotion Fixtures

use rustc_hash::FxHashMap;
use rusty_money::Money;
use serde::Deserialize;
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    discounts::SimpleDiscount,
    fixtures::{
        FixtureError,
        products::{parse_percentage, parse_price},
    },
    promotions::{
        Promotion, PromotionKey, PromotionMeta, PromotionSlotKey,
        budget::PromotionBudget,
        promotion,
        types::{
            DirectDiscountPromotion, MixAndMatchDiscount, MixAndMatchPromotion, MixAndMatchSlot,
            PositionalDiscountPromotion, ThresholdDiscount, ThresholdTier, TierThreshold,
            TieredThresholdPromotion,
        },
    },
    tags::string::StringTagCollection,
};

/// Wrapper for promotions in YAML
#[derive(Debug, Deserialize)]
pub struct PromotionsFixture {
    /// Map of promotion key -> promotion fixture
    pub promotions: FxHashMap<String, PromotionFixture>,
}

/// Budget constraint fixture
#[derive(Debug, Deserialize)]
pub struct BudgetFixture {
    /// Maximum applications (items or bundles)
    pub applications: Option<u32>,

    /// Maximum monetary discount value (e.g., "10.00 GBP")
    pub monetary: Option<String>,
}

impl BudgetFixture {
    fn try_into_budget(self) -> Result<PromotionBudget<'static>, FixtureError> {
        let monetary = if let Some(amount_str) = self.monetary {
            let (minor, currency) = parse_price(&amount_str)?;

            Some(Money::from_minor(minor, currency))
        } else {
            None
        };

        Ok(PromotionBudget {
            application_limit: self.applications,
            monetary_limit: monetary,
        })
    }
}

/// Promotion fixture from YAML
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromotionFixture {
    /// Direct Discount Promotion
    DirectDiscount {
        /// Promotion name
        name: String,

        /// Promotion tags for targeting
        tags: Vec<String>,

        /// Discount configuration
        discount: SimpleDiscountFixture,

        /// Budget constraints (optional)
        #[serde(default)]
        budget: Option<BudgetFixture>,
    },

    /// Mix-and-Match Bundle Promotion
    MixAndMatch {
        /// Promotion name
        name: String,

        /// Slot definitions
        slots: Vec<MixAndMatchSlotFixture>,

        /// Discount configuration
        discount: MixAndMatchDiscountFixture,

        /// Budget constraints (optional)
        #[serde(default)]
        budget: Option<BudgetFixture>,
    },

    /// Positional Discount Promotion
    PositionalDiscount {
        /// Promotion name
        name: String,

        /// Promotion tags
        tags: Vec<String>,

        /// Size of the bundle
        size: u16,

        /// The nth item in the bundle to apply the discount to
        positions: Vec<u16>,

        /// Discount configuration
        discount: SimpleDiscountFixture,

        /// Budget constraints (optional)
        #[serde(default)]
        budget: Option<BudgetFixture>,
    },

    /// Tiered Threshold Promotion
    TieredThreshold {
        /// Promotion name
        name: String,

        /// Tier definitions
        tiers: Vec<ThresholdTierFixture>,

        /// Budget constraints (optional)
        #[serde(default)]
        budget: Option<BudgetFixture>,
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
                budget,
            } => {
                let meta = PromotionMeta {
                    name: name.clone(),
                    slot_names: SecondaryMap::new(),
                    layer_names: SecondaryMap::new(),
                };

                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();

                let budget = budget
                    .map(BudgetFixture::try_into_budget)
                    .transpose()?
                    .unwrap_or_else(PromotionBudget::unlimited);

                let promotion = promotion(DirectDiscountPromotion::new(
                    key,
                    StringTagCollection::from_strs(&tag_refs),
                    SimpleDiscount::try_from(discount)?,
                    budget,
                ));

                Ok((meta, promotion))
            }
            PromotionFixture::MixAndMatch {
                name,
                slots,
                discount,
                budget,
            } => convert_mix_and_match(key, name, slots, discount, budget),
            Self::PositionalDiscount {
                name,
                tags,
                size,
                positions,
                discount,
                budget,
            } => {
                let meta = PromotionMeta {
                    name: name.clone(),
                    slot_names: SecondaryMap::new(),
                    layer_names: SecondaryMap::new(),
                };

                let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();

                let budget = budget
                    .map(BudgetFixture::try_into_budget)
                    .transpose()?
                    .unwrap_or_else(PromotionBudget::unlimited);

                let promotion = promotion(PositionalDiscountPromotion::new(
                    key,
                    StringTagCollection::from_strs(&tag_refs),
                    size,
                    positions.into(),
                    SimpleDiscount::try_from(discount)?,
                    budget,
                ));

                Ok((meta, promotion))
            }
            Self::TieredThreshold {
                name,
                tiers,
                budget,
            } => convert_tiered_threshold(key, &name, tiers, budget),
        }
    }
}

fn convert_mix_and_match(
    key: PromotionKey,
    name: String,
    slots: Vec<MixAndMatchSlotFixture>,
    discount: MixAndMatchDiscountFixture,
    budget: Option<BudgetFixture>,
) -> Result<(PromotionMeta, Promotion<'static>), FixtureError> {
    let mut slot_names = SecondaryMap::new();
    let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

    let slot_defs = slots
        .into_iter()
        .map(|slot| {
            let MixAndMatchSlotFixture {
                name,
                tags,
                min,
                max,
            } = slot;

            let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
            let slot_key = slot_keys.insert(());

            slot_names.insert(slot_key, name);

            MixAndMatchSlot::new(
                slot_key,
                StringTagCollection::from_strs(&tag_refs),
                min,
                max,
            )
        })
        .collect();

    let meta = PromotionMeta {
        name,
        slot_names,
        layer_names: SecondaryMap::new(),
    };

    let budget = budget
        .map(BudgetFixture::try_into_budget)
        .transpose()?
        .unwrap_or_else(PromotionBudget::unlimited);

    let promo = promotion(MixAndMatchPromotion::new(
        key,
        slot_defs,
        MixAndMatchDiscount::try_from(discount)?,
        budget,
    ));

    Ok((meta, promo))
}

fn convert_tiered_threshold(
    key: PromotionKey,
    name: &str,
    tiers: Vec<ThresholdTierFixture>,
    budget: Option<BudgetFixture>,
) -> Result<(PromotionMeta, Promotion<'static>), FixtureError> {
    let meta = PromotionMeta {
        name: name.to_string(),
        slot_names: SecondaryMap::new(),
        layer_names: SecondaryMap::new(),
    };

    let budget = budget
        .map(BudgetFixture::try_into_budget)
        .transpose()?
        .unwrap_or_else(PromotionBudget::unlimited);

    let tier_defs: Vec<ThresholdTier<'static>> = tiers
        .into_iter()
        .map(|tier_fixture| {
            let ThresholdTierFixture {
                lower_threshold,
                upper_threshold,
                contribution_tags,
                discount_tags,
                discount,
            } = tier_fixture;

            let lower_threshold = lower_threshold.ok_or_else(|| {
                FixtureError::InvalidPromotionData(
                    "tier threshold must define lower_threshold".to_string(),
                )
            })?;
            let lower_threshold = parse_threshold_requirements(lower_threshold, "lower_threshold")?;
            let upper_threshold = upper_threshold
                .map(|threshold| parse_threshold_requirements(threshold, "upper_threshold"))
                .transpose()?;

            let contribution_tag_refs: Vec<&str> =
                contribution_tags.iter().map(String::as_str).collect();

            let discount_tag_refs: Vec<&str> = discount_tags.iter().map(String::as_str).collect();

            let contribution_tags = StringTagCollection::from_strs(&contribution_tag_refs);
            let discount_tags = StringTagCollection::from_strs(&discount_tag_refs);
            let discount = ThresholdDiscount::try_from(discount)?;

            Ok(ThresholdTier::with_thresholds(
                lower_threshold,
                upper_threshold,
                contribution_tags,
                discount_tags,
                discount,
            ))
        })
        .collect::<Result<Vec<_>, FixtureError>>()?;

    let promo = promotion(TieredThresholdPromotion::new(key, tier_defs, budget));

    Ok((meta, promo))
}

/// Threshold tier definition from YAML fixtures
#[derive(Debug, Deserialize)]
pub struct ThresholdTierFixture {
    /// Lower threshold requirements.
    #[serde(default)]
    pub lower_threshold: Option<ThresholdRequirementsFixture>,

    /// Optional upper threshold requirements.
    #[serde(default)]
    pub upper_threshold: Option<ThresholdRequirementsFixture>,

    /// Tags for items that contribute to the threshold
    #[serde(default)]
    pub contribution_tags: Vec<String>,

    /// Tags for items that receive the discount
    #[serde(default)]
    pub discount_tags: Vec<String>,

    /// Discount configuration
    pub discount: ThresholdDiscountFixture,
}

/// Threshold requirements from YAML fixtures.
#[derive(Debug, Default, Deserialize)]
pub struct ThresholdRequirementsFixture {
    /// Optional spend threshold (e.g., "30.00 GBP")
    #[serde(default)]
    pub monetary: Option<String>,

    /// Optional minimum number of contributing items required.
    #[serde(default)]
    pub items: Option<u32>,
}

fn parse_threshold_requirements(
    threshold: ThresholdRequirementsFixture,
    field_name: &str,
) -> Result<TierThreshold<'static>, FixtureError> {
    let monetary_threshold = threshold
        .monetary
        .map(|monetary| parse_price(&monetary))
        .transpose()?
        .map(|(threshold_minor, threshold_currency)| {
            Money::from_minor(threshold_minor, threshold_currency)
        });

    let item_count_threshold = threshold.items;

    match (monetary_threshold, item_count_threshold) {
        (Some(monetary_threshold), Some(item_count_threshold)) => Ok(
            TierThreshold::with_both_thresholds(monetary_threshold, item_count_threshold),
        ),
        (Some(monetary_threshold), None) => {
            Ok(TierThreshold::with_monetary_threshold(monetary_threshold))
        }
        (None, Some(item_count_threshold)) => Ok(TierThreshold::with_item_count_threshold(
            item_count_threshold,
        )),
        (None, None) => Err(FixtureError::InvalidPromotionData(format!(
            "tier threshold must define {field_name}.monetary and/or {field_name}.items"
        ))),
    }
}

/// Simple Discount configuration from YAML fixtures
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimpleDiscountFixture {
    /// Percentage discount (supports "15%" or "0.15" formats)
    PercentageOff {
        /// Discount percentage (e.g., "15%" or "0.15" for 15%)
        amount: String,
    },

    /// Fixed price override (e.g., "2.50 GBP")
    AmountOverride {
        /// Price string (e.g., "2.50 GBP")
        amount: String,
    },

    /// Fixed amount discount off (e.g., "0.75 GBP")
    AmountOff {
        /// Discount amount string (e.g., "0.75 GBP")
        amount: String,
    },
}

/// Mix-and-Match discount configuration from YAML fixtures
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MixAndMatchDiscountFixture {
    /// Percentage discount applied to all items
    PercentAllItems {
        /// Discount percentage (e.g., "15%" or "0.15" for 15%)
        amount: String,
    },

    /// Fixed amount subtracted from each item in the bundle
    AmountOffEachItem {
        /// Discount amount string (e.g., "0.75 GBP")
        amount: String,
    },

    /// Each item in the bundle is set to a fixed price
    FixedPriceEachItem {
        /// Price string (e.g., "2.50 GBP")
        amount: String,
    },

    /// Fixed amount subtracted from the total bundle price
    AmountOffTotal {
        /// Discount amount string (e.g., "5.00 GBP")
        amount: String,
    },

    /// Percentage discount applied to the cheapest item
    PercentCheapest {
        /// Discount percentage (e.g., "15%" or "0.15" for 15%)
        amount: String,
    },

    /// Fixed total price for the bundle
    FixedTotal {
        /// Price string (e.g., "2.50 GBP")
        amount: String,
    },

    /// Fixed price applied to the cheapest item
    FixedCheapest {
        /// Price string (e.g., "0.99 GBP")
        amount: String,
    },
}

impl TryFrom<MixAndMatchDiscountFixture> for MixAndMatchDiscount<'_> {
    type Error = FixtureError;

    fn try_from(config: MixAndMatchDiscountFixture) -> Result<Self, Self::Error> {
        match config {
            MixAndMatchDiscountFixture::PercentAllItems { amount } => Ok(
                MixAndMatchDiscount::PercentAllItems(parse_percentage(&amount)?),
            ),
            MixAndMatchDiscountFixture::AmountOffEachItem { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(MixAndMatchDiscount::AmountOffEachItem(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            MixAndMatchDiscountFixture::FixedPriceEachItem { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(MixAndMatchDiscount::FixedPriceEachItem(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            MixAndMatchDiscountFixture::AmountOffTotal { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(MixAndMatchDiscount::AmountOffTotal(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            MixAndMatchDiscountFixture::PercentCheapest { amount } => Ok(
                MixAndMatchDiscount::PercentCheapest(parse_percentage(&amount)?),
            ),
            MixAndMatchDiscountFixture::FixedTotal { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(MixAndMatchDiscount::FixedTotal(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            MixAndMatchDiscountFixture::FixedCheapest { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(MixAndMatchDiscount::FixedCheapest(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
        }
    }
}

/// Threshold discount configuration from YAML fixtures.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThresholdDiscountFixture {
    /// Percentage discount applied independently to each eligible item.
    PercentEachItem {
        /// Discount percentage (e.g., "15%" or "0.15" for 15%)
        amount: String,
    },

    /// Fixed amount subtracted from each eligible item's price.
    AmountOffEachItem {
        /// Discount amount string (e.g., "0.75 GBP")
        amount: String,
    },

    /// Each eligible item's price is overridden to a fixed amount.
    FixedPriceEachItem {
        /// Price string (e.g., "2.50 GBP")
        amount: String,
    },

    /// Fixed amount subtracted from the total of all eligible items.
    AmountOffTotal {
        /// Discount amount string (e.g., "5.00 GBP")
        amount: String,
    },

    /// All eligible items together cost a fixed total.
    FixedTotal {
        /// Price string (e.g., "10.00 GBP")
        amount: String,
    },

    /// Percentage discount applied only to the cheapest eligible item.
    PercentCheapest {
        /// Discount percentage (e.g., "50%" or "0.50" for 50%)
        amount: String,
    },

    /// The cheapest eligible item's price is set to a fixed amount.
    FixedCheapest {
        /// Price string (e.g., "0.99 GBP")
        amount: String,
    },
}

impl TryFrom<ThresholdDiscountFixture> for ThresholdDiscount<'_> {
    type Error = FixtureError;

    fn try_from(config: ThresholdDiscountFixture) -> Result<Self, Self::Error> {
        match config {
            ThresholdDiscountFixture::PercentEachItem { amount } => Ok(
                ThresholdDiscount::PercentEachItem(parse_percentage(&amount)?),
            ),
            ThresholdDiscountFixture::AmountOffEachItem { amount } => {
                let (minor, currency) = parse_price(&amount)?;

                Ok(ThresholdDiscount::AmountOffEachItem(Money::from_minor(
                    minor, currency,
                )))
            }
            ThresholdDiscountFixture::FixedPriceEachItem { amount } => {
                let (minor, currency) = parse_price(&amount)?;

                Ok(ThresholdDiscount::FixedPriceEachItem(Money::from_minor(
                    minor, currency,
                )))
            }
            ThresholdDiscountFixture::AmountOffTotal { amount } => {
                let (minor, currency) = parse_price(&amount)?;

                Ok(ThresholdDiscount::AmountOffTotal(Money::from_minor(
                    minor, currency,
                )))
            }
            ThresholdDiscountFixture::FixedTotal { amount } => {
                let (minor, currency) = parse_price(&amount)?;

                Ok(ThresholdDiscount::FixedTotal(Money::from_minor(
                    minor, currency,
                )))
            }
            ThresholdDiscountFixture::PercentCheapest { amount } => Ok(
                ThresholdDiscount::PercentCheapest(parse_percentage(&amount)?),
            ),
            ThresholdDiscountFixture::FixedCheapest { amount } => {
                let (minor, currency) = parse_price(&amount)?;

                Ok(ThresholdDiscount::FixedCheapest(Money::from_minor(
                    minor, currency,
                )))
            }
        }
    }
}

/// Slot definition for mix-and-match fixtures.
#[derive(Debug, Deserialize)]
pub struct MixAndMatchSlotFixture {
    /// Slot name
    pub name: String,

    /// Slot tags (OR)
    pub tags: Vec<String>,

    /// Minimum required items
    pub min: usize,

    /// Maximum allowed items
    pub max: Option<usize>,
}

impl TryFrom<SimpleDiscountFixture> for SimpleDiscount<'_> {
    type Error = FixtureError;

    fn try_from(config: SimpleDiscountFixture) -> Result<Self, Self::Error> {
        match config {
            SimpleDiscountFixture::PercentageOff { amount: percentage } => Ok(
                SimpleDiscount::PercentageOff(parse_percentage(&percentage)?),
            ),
            SimpleDiscountFixture::AmountOverride { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(SimpleDiscount::AmountOverride(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
            SimpleDiscountFixture::AmountOff { amount } => {
                let (minor_units, currency) = parse_price(&amount)?;

                Ok(SimpleDiscount::AmountOff(Money::from_minor(
                    minor_units,
                    currency,
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use rusty_money::iso::GBP;
    use slotmap::SlotMap;
    use testresult::TestResult;

    use crate::{discounts::SimpleDiscount, promotions::PromotionKey};

    use super::*;

    fn test_promotion_key() -> PromotionKey {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        keys.insert(())
    }

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
        let fixture = SimpleDiscountFixture::PercentageOff {
            amount: "15%".to_string(),
        };

        let config = SimpleDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            SimpleDiscount::PercentageOff(percent) if percent == Percentage::from(0.15)
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_parses_percentage_decimal_format() -> Result<(), FixtureError> {
        let fixture = SimpleDiscountFixture::PercentageOff {
            amount: "0.15".to_string(),
        };

        let config = SimpleDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            SimpleDiscount::PercentageOff(percent) if percent == Percentage::from(0.15)
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_parses_amount_override() -> Result<(), FixtureError> {
        let fixture = SimpleDiscountFixture::AmountOverride {
            amount: "2.50 GBP".to_string(),
        };

        let config = SimpleDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            SimpleDiscount::AmountOverride(money) if money.to_minor_units() == 250
                && money.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn discount_fixture_parses_amount_discount_off() -> Result<(), FixtureError> {
        let fixture = SimpleDiscountFixture::AmountOff {
            amount: "0.75 GBP".to_string(),
        };

        let config = SimpleDiscount::try_from(fixture)?;

        assert!(matches!(
            config,
            SimpleDiscount::AmountOff(money) if money.to_minor_units() == 75
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
        let result: Result<SimpleDiscountFixture, _> = serde_norway::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn discount_fixture_rejects_invalid_percentage_string() {
        let fixture = SimpleDiscountFixture::PercentageOff {
            amount: "not a number".to_string(),
        };

        let result = SimpleDiscount::try_from(fixture);
        assert!(matches!(result, Err(FixtureError::InvalidPercentage(_))));
    }

    #[test]
    fn promotion_fixture_converts_direct_discount() -> TestResult {
        let fixture = PromotionFixture::DirectDiscount {
            name: "Member Sale".to_string(),
            tags: vec!["member".to_string(), "sale".to_string()],
            discount: SimpleDiscountFixture::AmountOff {
                amount: "0.50 GBP".to_string(),
            },
            budget: None,
        };

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Member Sale");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn promotion_fixture_converts_positional_discount() -> TestResult {
        let fixture = PromotionFixture::PositionalDiscount {
            name: "3-for-2".to_string(),
            tags: vec!["snack".to_string()],
            size: 3,
            positions: vec![2],
            discount: SimpleDiscountFixture::PercentageOff {
                amount: "50%".to_string(),
            },
            budget: None,
        };

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "3-for-2");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn promotion_fixture_converts_mix_and_match() -> TestResult {
        let fixture = PromotionFixture::MixAndMatch {
            name: "Meal Deal".to_string(),
            slots: vec![
                MixAndMatchSlotFixture {
                    name: "main".to_string(),
                    tags: vec!["main".to_string()],
                    min: 1,
                    max: Some(1),
                },
                MixAndMatchSlotFixture {
                    name: "drink".to_string(),
                    tags: vec!["drink".to_string()],
                    min: 1,
                    max: Some(1),
                },
            ],
            discount: MixAndMatchDiscountFixture::FixedTotal {
                amount: "2.50 GBP".to_string(),
            },
            budget: None,
        };

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Meal Deal");
        assert_eq!(promotion.key(), key);
        assert_eq!(meta.slot_names.len(), 2);

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_percent_all_items() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::PercentAllItems {
            amount: "25%".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::PercentAllItems(pct) if pct == Percentage::from(0.25)
        ));

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_percent_cheapest() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::PercentCheapest {
            amount: "50%".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::PercentCheapest(pct) if pct == Percentage::from(0.50)
        ));

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_amount_off_each_item() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::AmountOffEachItem {
            amount: "0.75 GBP".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::AmountOffEachItem(amount)
                if amount.to_minor_units() == 75 && amount.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_fixed_price_each_item() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::FixedPriceEachItem {
            amount: "2.50 GBP".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::FixedPriceEachItem(amount)
                if amount.to_minor_units() == 250 && amount.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_amount_off_total() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::AmountOffTotal {
            amount: "1.00 GBP".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::AmountOffTotal(amount)
                if amount.to_minor_units() == 100 && amount.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn mix_and_match_discount_parses_fixed_cheapest() -> TestResult {
        let fixture = MixAndMatchDiscountFixture::FixedCheapest {
            amount: "0.99 GBP".to_string(),
        };

        let discount = MixAndMatchDiscount::try_from(fixture)?;

        assert!(matches!(
            discount,
            MixAndMatchDiscount::FixedCheapest(amount)
                if amount.to_minor_units() == 99 && amount.currency() == GBP
        ));

        Ok(())
    }

    #[test]
    fn budget_fixture_parses_application_limit() -> Result<(), FixtureError> {
        let budget_fixture = BudgetFixture {
            applications: Some(5),
            monetary: None,
        };

        let budget = budget_fixture.try_into_budget()?;

        assert_eq!(budget.application_limit, Some(5));
        assert!(budget.monetary_limit.is_none());

        Ok(())
    }

    #[test]
    fn budget_fixture_parses_monetary_limit() -> Result<(), FixtureError> {
        let budget_fixture = BudgetFixture {
            applications: None,
            monetary: Some("2.50 GBP".to_string()),
        };

        let budget = budget_fixture.try_into_budget()?;

        assert!(budget.application_limit.is_none());
        assert_eq!(budget.monetary_limit, Some(Money::from_minor(250, GBP)));

        Ok(())
    }

    #[test]
    fn budget_fixture_parses_both_limits() -> Result<(), FixtureError> {
        let budget_fixture = BudgetFixture {
            applications: Some(10),
            monetary: Some("5.00 GBP".to_string()),
        };

        let budget = budget_fixture.try_into_budget()?;

        assert_eq!(budget.application_limit, Some(10));
        assert_eq!(budget.monetary_limit, Some(Money::from_minor(500, GBP)));

        Ok(())
    }

    #[test]
    fn budget_fixture_parses_neither_limit() -> Result<(), FixtureError> {
        let budget_fixture = BudgetFixture {
            applications: None,
            monetary: None,
        };

        let budget = budget_fixture.try_into_budget()?;

        assert!(budget.application_limit.is_none());
        assert!(budget.monetary_limit.is_none());

        Ok(())
    }

    #[test]
    fn promotion_fixture_direct_discount_with_budget() -> TestResult {
        let fixture = PromotionFixture::DirectDiscount {
            name: "Sale with Budget".to_string(),
            tags: vec!["item".to_string()],
            discount: SimpleDiscountFixture::PercentageOff {
                amount: "25%".to_string(),
            },
            budget: Some(BudgetFixture {
                applications: Some(3),
                monetary: Some("1.00 GBP".to_string()),
            }),
        };

        let key = test_promotion_key();
        let (_meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn promotion_fixture_positional_discount_with_budget() -> TestResult {
        let fixture = PromotionFixture::PositionalDiscount {
            name: "BOGOF Limited".to_string(),
            tags: vec!["snack".to_string()],
            size: 2,
            positions: vec![1],
            discount: SimpleDiscountFixture::PercentageOff {
                amount: "100%".to_string(),
            },
            budget: Some(BudgetFixture {
                applications: Some(5),
                monetary: None,
            }),
        };

        let key = test_promotion_key();
        let (_meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn promotion_fixture_converts_tiered_threshold() -> TestResult {
        let fixture = PromotionFixture::TieredThreshold {
            name: "Wine & Cheese Deal".to_string(),
            tiers: vec![ThresholdTierFixture {
                lower_threshold: Some(ThresholdRequirementsFixture {
                    monetary: Some("30.00 GBP".to_string()),
                    items: None,
                }),
                upper_threshold: None,
                contribution_tags: vec!["wine".to_string()],
                discount_tags: vec!["cheese".to_string()],
                discount: ThresholdDiscountFixture::PercentEachItem {
                    amount: "10%".to_string(),
                },
            }],
            budget: None,
        };

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Wine & Cheese Deal");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn promotion_fixture_tiered_threshold_with_budget() -> TestResult {
        let fixture = PromotionFixture::TieredThreshold {
            name: "Tiered with Budget".to_string(),
            tiers: vec![ThresholdTierFixture {
                lower_threshold: Some(ThresholdRequirementsFixture {
                    monetary: Some("50.00 GBP".to_string()),
                    items: None,
                }),
                upper_threshold: None,
                contribution_tags: vec![],
                discount_tags: vec![],
                discount: ThresholdDiscountFixture::AmountOffEachItem {
                    amount: "5.00 GBP".to_string(),
                },
            }],
            budget: Some(BudgetFixture {
                applications: Some(3),
                monetary: Some("10.00 GBP".to_string()),
            }),
        };

        let key = test_promotion_key();
        let (_meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn tiered_threshold_fixture_yaml_round_trip() -> TestResult {
        let yaml = r"
type: tiered_threshold
name: Spend & Save
tiers:
  - lower_threshold:
      monetary: '50.00 GBP'
    contribution_tags: []
    discount_tags: []
    discount:
      type: amount_off_each_item
      amount: '5.00 GBP'
  - lower_threshold:
      monetary: '80.00 GBP'
    contribution_tags: []
    discount_tags: []
    discount:
      type: amount_off_each_item
      amount: '12.00 GBP'
";
        let fixture: PromotionFixture = serde_norway::from_str(yaml)?;

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Spend & Save");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn tiered_threshold_fixture_supports_item_count_threshold() -> TestResult {
        let yaml = r"
type: tiered_threshold
name: Spend & Count
tiers:
  - lower_threshold:
      monetary: '20.00 GBP'
      items: 3
    contribution_tags: []
    discount_tags: []
    discount:
      type: percent_each_item
      amount: '10%'
";
        let fixture: PromotionFixture = serde_norway::from_str(yaml)?;

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Spend & Count");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn tiered_threshold_fixture_supports_item_count_only_threshold() -> TestResult {
        let yaml = r"
type: tiered_threshold
name: Count Only
tiers:
  - lower_threshold:
      items: 3
    contribution_tags: []
    discount_tags: []
    discount:
      type: percent_each_item
      amount: '10%'
";
        let fixture: PromotionFixture = serde_norway::from_str(yaml)?;

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Count Only");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn tiered_threshold_fixture_supports_upper_threshold() -> TestResult {
        let yaml = r"
type: tiered_threshold
name: Lower and Upper
tiers:
  - lower_threshold:
      monetary: '20.00 GBP'
    upper_threshold:
      monetary: '60.00 GBP'
      items: 5
    contribution_tags: []
    discount_tags: []
    discount:
      type: percent_each_item
      amount: '10%'
";
        let fixture: PromotionFixture = serde_norway::from_str(yaml)?;

        let key = test_promotion_key();
        let (meta, promotion) = fixture.try_into_promotion(key)?;

        assert_eq!(meta.name, "Lower and Upper");
        assert_eq!(promotion.key(), key);

        Ok(())
    }

    #[test]
    fn tiered_threshold_fixture_rejects_empty_upper_threshold_definition() {
        let yaml = r"
type: tiered_threshold
name: Empty Upper
tiers:
  - lower_threshold:
      monetary: '20.00 GBP'
    upper_threshold: {}
    contribution_tags: []
    discount_tags: []
    discount:
      type: percent_each_item
      amount: '10%'
";
        let fixture: Result<PromotionFixture, _> = serde_norway::from_str(yaml);
        let Ok(fixture) = fixture else {
            panic!("Fixture YAML should parse before semantic validation");
        };

        let key = test_promotion_key();
        let result = fixture.try_into_promotion(key);

        assert!(result.is_err());
    }

    #[test]
    fn tiered_threshold_fixture_rejects_empty_threshold_definition() {
        let yaml = r"
type: tiered_threshold
name: Empty Threshold
tiers:
  - contribution_tags: []
    discount_tags: []
    discount:
      type: percent_each_item
      amount: '10%'
";
        let fixture: Result<PromotionFixture, _> = serde_norway::from_str(yaml);
        let Ok(fixture) = fixture else {
            panic!("Fixture YAML should parse before semantic validation");
        };

        let key = test_promotion_key();
        let result = fixture.try_into_promotion(key);

        assert!(result.is_err());
    }

    #[test]
    fn tiered_threshold_fixture_rejects_unknown_tier_discount_type() {
        let yaml = r"
type: tiered_threshold
name: Bad Tier
tiers:
  - lower_threshold:
      monetary: '50.00 GBP'
    contribution_tags: []
    discount_tags: []
    discount:
      type: mystery_discount
      amount: '5.00'
";
        let result: Result<PromotionFixture, _> = serde_norway::from_str(yaml);

        assert!(result.is_err());
    }
}
