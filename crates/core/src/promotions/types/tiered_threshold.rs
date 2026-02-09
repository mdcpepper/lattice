//! Tiered Threshold Promotion
//!
//! A promotion that checks whether items matching `contribution_tags` meet
//! threshold requirements, then applies a [`ThresholdDiscount`] to items matching
//! `discount_tags`. Multiple tiers can be defined (e.g., spend £50 for 5% off,
//! spend £80 for 12% off); the ILP solver selects the single best tier that
//! minimises total basket cost.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::Currency};

use crate::{
    discounts::{DiscountError, percent_of_minor},
    items::Item,
    promotions::{PromotionKey, budget::PromotionBudget, qualification::Qualification},
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// Discount variants for tiered threshold promotions.
///
/// Per-item variants apply their discount independently to each eligible item.
/// Bundle-level variants apply the discount across the group of eligible items
/// as a whole, with the ILP solver handling pricing allocation.
#[derive(Debug, Clone)]
pub enum ThresholdDiscount<'a> {
    /// Percentage discount applied independently to each eligible item.
    PercentEachItem(Percentage),

    /// Fixed amount subtracted from each eligible item's price.
    AmountOffEachItem(Money<'a, Currency>),

    /// Each eligible item's price is overridden to a fixed amount.
    FixedPriceEachItem(Money<'a, Currency>),

    /// Fixed amount subtracted from the total of all eligible items.
    AmountOffTotal(Money<'a, Currency>),

    /// All eligible items together cost a fixed total.
    FixedTotal(Money<'a, Currency>),

    /// Percentage discount applied only to the cheapest eligible item.
    PercentCheapest(Percentage),

    /// The cheapest eligible item's price is set to a fixed amount.
    FixedCheapest(Money<'a, Currency>),
}

/// Threshold requirements for a tier.
///
/// A threshold can require spend, item count, or both.
#[derive(Debug, Clone)]
pub struct TierThreshold<'a> {
    monetary_threshold: Option<Money<'a, Currency>>,
    item_count_threshold: Option<u32>,
}

impl<'a> TierThreshold<'a> {
    /// Create a new threshold with monetary and/or item count requirements, or none.
    pub fn new(
        monetary_threshold: Option<Money<'a, Currency>>,
        item_count_threshold: Option<u32>,
    ) -> Self {
        Self {
            monetary_threshold,
            item_count_threshold,
        }
    }

    /// Create a threshold with a monetary requirement only.
    pub fn with_monetary_threshold(monetary: Money<'a, Currency>) -> Self {
        Self {
            monetary_threshold: Some(monetary),
            item_count_threshold: None,
        }
    }

    /// Create a threshold with an item-count requirement only.
    pub const fn with_item_count_threshold(item_count: u32) -> Self {
        Self {
            monetary_threshold: None,
            item_count_threshold: Some(item_count),
        }
    }

    /// Create a threshold with both monetary and item-count requirements.
    pub fn with_both_thresholds(monetary: Money<'a, Currency>, item_count: u32) -> Self {
        Self {
            monetary_threshold: Some(monetary),
            item_count_threshold: Some(item_count),
        }
    }

    /// Return the optional monetary threshold.
    pub fn monetary_threshold(&self) -> Option<&Money<'a, Currency>> {
        self.monetary_threshold.as_ref()
    }

    /// Return the optional item-count threshold.
    pub const fn item_count_threshold(&self) -> Option<u32> {
        self.item_count_threshold
    }
}

/// A single threshold tier within a tiered threshold promotion.
///
/// Each tier specifies lower threshold requirements, an optional upper
/// threshold cap, which items contribute to qualification, which items receive
/// the discount, and what discount applies.
#[derive(Debug, Clone)]
pub struct ThresholdTier<'a, T: TagCollection = StringTagCollection> {
    lower_threshold: TierThreshold<'a>,
    upper_threshold: Option<TierThreshold<'a>>,
    contribution_qualification: Qualification<T>,
    discount_qualification: Qualification<T>,
    discount: ThresholdDiscount<'a>,
}

impl<'a, T: TagCollection> ThresholdTier<'a, T> {
    /// Create a new threshold tier from validated lower and optional upper thresholds.
    pub fn new(
        lower_threshold: TierThreshold<'a>,
        upper_threshold: Option<TierThreshold<'a>>,
        contribution_qualification: Qualification<T>,
        discount_qualification: Qualification<T>,
        discount: ThresholdDiscount<'a>,
    ) -> Self {
        Self {
            lower_threshold,
            upper_threshold,
            contribution_qualification,
            discount_qualification,
            discount,
        }
    }

    /// Return lower threshold requirements.
    pub const fn lower_threshold(&self) -> &TierThreshold<'a> {
        &self.lower_threshold
    }

    /// Return optional upper threshold requirements.
    pub const fn upper_threshold(&self) -> Option<&TierThreshold<'a>> {
        self.upper_threshold.as_ref()
    }

    /// Return the contribution qualification.
    pub fn contribution_qualification(&self) -> &Qualification<T> {
        &self.contribution_qualification
    }

    /// Return the discount qualification.
    pub fn discount_qualification(&self) -> &Qualification<T> {
        &self.discount_qualification
    }

    /// Return the discount.
    pub fn discount(&self) -> &ThresholdDiscount<'a> {
        &self.discount
    }
}

/// A tiered threshold promotion.
///
/// Evaluates items against tier requirements (spend plus optional item count):
/// if items matching `contribution_tags` meet a tier's thresholds, items matching
/// `discount_tags` receive the tier's discount. The ILP solver picks the single
/// best qualifying tier to minimise total cost.
#[derive(Debug, Clone)]
pub struct TieredThresholdPromotion<'a, T: TagCollection = StringTagCollection> {
    key: PromotionKey,
    tiers: Vec<ThresholdTier<'a, T>>,
    budget: PromotionBudget<'a>,
}

impl<'a, T: TagCollection> TieredThresholdPromotion<'a, T> {
    /// Create a new tiered threshold promotion.
    pub fn new(
        key: PromotionKey,
        tiers: Vec<ThresholdTier<'a, T>>,
        budget: PromotionBudget<'a>,
    ) -> Self {
        Self { key, tiers, budget }
    }

    /// Return the promotion key.
    pub fn key(&self) -> PromotionKey {
        self.key
    }

    /// Return the tiers.
    pub fn tiers(&self) -> &[ThresholdTier<'a, T>] {
        &self.tiers
    }

    /// Return the budget.
    pub const fn budget(&self) -> &PromotionBudget<'a> {
        &self.budget
    }

    /// Calculate the discounted price for a single item under a per-item discount.
    ///
    /// For per-item discount variants ([`PercentEachItem`](ThresholdDiscount::PercentEachItem),
    /// [`AmountOffEachItem`](ThresholdDiscount::AmountOffEachItem),
    /// [`FixedPriceEachItem`](ThresholdDiscount::FixedPriceEachItem)), this computes the
    /// discounted price. For bundle-level variants the item's original price is
    /// returned unchanged because the effective per-item price depends on the
    /// full set of participating items and is computed by the ILP solver.
    ///
    /// # Errors
    ///
    /// Returns a [`DiscountError`] if:
    /// - Percentage calculation overflows or cannot be safely represented.
    /// - Money arithmetic fails (e.g., currency mismatch, negative result).
    pub fn calculate_discounted_price(
        tier: &ThresholdTier<'a, T>,
        item: &Item<'a, T>,
    ) -> Result<Money<'a, Currency>, DiscountError> {
        let discounted_minor = match &tier.discount {
            ThresholdDiscount::PercentEachItem(pct) => {
                let original_minor = item.price().to_minor_units();

                original_minor
                    .checked_sub(percent_of_minor(pct, original_minor)?)
                    .ok_or(DiscountError::PercentConversion)?
            }
            ThresholdDiscount::FixedPriceEachItem(amount) => amount.to_minor_units(),
            ThresholdDiscount::AmountOffEachItem(amount) => {
                item.price().sub(*amount)?.to_minor_units()
            }
            // Bundle-level variants: per-item price is the full price
            ThresholdDiscount::AmountOffTotal(_)
            | ThresholdDiscount::FixedTotal(_)
            | ThresholdDiscount::PercentCheapest(_)
            | ThresholdDiscount::FixedCheapest(_) => item.price().to_minor_units(),
        };

        Ok(Money::from_minor(
            0.max(discounted_minor),
            item.price().currency(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;
    use testresult::TestResult;

    use crate::{items::Item, products::ProductKey};

    use super::*;

    fn make_tier(
        threshold_minor: i64,
        discount: ThresholdDiscount<'_>,
    ) -> ThresholdTier<'_, StringTagCollection> {
        ThresholdTier::new(
            TierThreshold::with_monetary_threshold(Money::from_minor(threshold_minor, GBP)),
            None,
            Qualification::match_any(StringTagCollection::from_strs(&["wine"])),
            Qualification::match_any(StringTagCollection::from_strs(&["cheese"])),
            discount,
        )
    }

    #[test]
    fn key_returns_constructor_key() {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let promo = TieredThresholdPromotion::new(
            key,
            vec![make_tier(
                1000,
                ThresholdDiscount::FixedPriceEachItem(Money::from_minor(0, GBP)),
            )],
            PromotionBudget::unlimited(),
        );

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());
    }

    #[test]
    fn accessors_return_constructor_values() {
        let tier = make_tier(
            5000,
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        );

        let promo = TieredThresholdPromotion::new(
            PromotionKey::default(),
            vec![tier],
            PromotionBudget::unlimited(),
        );

        assert_eq!(promo.tiers().len(), 1);
        assert_eq!(
            promo
                .tiers()
                .first()
                .and_then(|t| t.lower_threshold().monetary_threshold())
                .map(Money::to_minor_units),
            Some(5000)
        );
        assert!(promo.tiers().first().is_some_and(|t| {
            t.contribution_qualification()
                .matches(&StringTagCollection::from_strs(&["wine"]))
        }));
        assert!(promo.tiers().first().is_some_and(|t| {
            t.discount_qualification()
                .matches(&StringTagCollection::from_strs(&["cheese"]))
        }));
        assert_eq!(
            promo
                .tiers()
                .first()
                .and_then(|t| t.lower_threshold().item_count_threshold()),
            None
        );
        assert!(
            promo
                .tiers()
                .first()
                .and_then(ThresholdTier::upper_threshold)
                .is_none()
        );
    }

    #[test]
    fn item_count_threshold_accessor_returns_configured_value() {
        let tier = ThresholdTier::new(
            TierThreshold::with_both_thresholds(Money::from_minor(5000, GBP), 3),
            None,
            Qualification::match_any(StringTagCollection::from_strs(&["wine"])),
            Qualification::match_any(StringTagCollection::from_strs(&["cheese"])),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        );

        assert_eq!(
            tier.lower_threshold()
                .monetary_threshold()
                .map(Money::to_minor_units),
            Some(5000)
        );
        assert_eq!(tier.lower_threshold().item_count_threshold(), Some(3));
    }

    #[test]
    fn tier_threshold_constructors_cover_all_supported_shapes() {
        let monetary_only = TierThreshold::with_monetary_threshold(Money::from_minor(5000, GBP));

        assert_eq!(
            monetary_only
                .monetary_threshold()
                .map(Money::to_minor_units),
            Some(5000)
        );

        assert_eq!(monetary_only.item_count_threshold(), None);

        let item_count_only = TierThreshold::with_item_count_threshold(3);

        assert_eq!(item_count_only.monetary_threshold(), None);
        assert_eq!(item_count_only.item_count_threshold(), Some(3));

        let both = TierThreshold::with_both_thresholds(Money::from_minor(5000, GBP), 3);

        assert_eq!(
            both.monetary_threshold().map(Money::to_minor_units),
            Some(5000)
        );
        assert_eq!(both.item_count_threshold(), Some(3));
    }

    #[test]
    fn item_count_only_threshold_constructor_sets_count_without_monetary() {
        let tier = ThresholdTier::new(
            TierThreshold::with_item_count_threshold(3),
            None,
            Qualification::match_any(StringTagCollection::from_strs(&["wine"])),
            Qualification::match_any(StringTagCollection::from_strs(&["cheese"])),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        );

        assert_eq!(tier.lower_threshold().monetary_threshold(), None);
        assert_eq!(tier.lower_threshold().item_count_threshold(), Some(3));
    }

    #[test]
    fn new_supports_optional_upper_threshold() {
        let tier = ThresholdTier::new(
            TierThreshold::with_both_thresholds(Money::from_minor(3000, GBP), 2),
            Some(TierThreshold::with_both_thresholds(
                Money::from_minor(6000, GBP),
                4,
            )),
            Qualification::match_any(StringTagCollection::from_strs(&["wine"])),
            Qualification::match_any(StringTagCollection::from_strs(&["wine"])),
            ThresholdDiscount::PercentEachItem(Percentage::from(0.10)),
        );

        assert_eq!(
            tier.lower_threshold()
                .monetary_threshold()
                .map(Money::to_minor_units),
            Some(3000)
        );
        assert_eq!(tier.lower_threshold().item_count_threshold(), Some(2));
        assert_eq!(
            tier.upper_threshold()
                .and_then(TierThreshold::monetary_threshold)
                .map(Money::to_minor_units),
            Some(6000)
        );
        assert_eq!(
            tier.upper_threshold()
                .and_then(TierThreshold::item_count_threshold),
            Some(4)
        );
    }

    #[test]
    fn calculate_discounted_price_percentage() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::PercentEachItem(Percentage::from(0.25)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(75, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_fixed_price() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::FixedPriceEachItem(Money::from_minor(50, GBP)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_amount_off() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::AmountOffEachItem(Money::from_minor(25, GBP)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(75, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_percentage_to_zero() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::PercentEachItem(Percentage::from(2.0)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_amount_off_to_zero() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::AmountOffEachItem(Money::from_minor(200, GBP)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_fixed_price_to_zero() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::FixedPriceEachItem(Money::from_minor(-50, GBP)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn bundle_level_discount_returns_full_price() -> TestResult {
        let tier = make_tier(
            1000,
            ThresholdDiscount::AmountOffTotal(Money::from_minor(500, GBP)),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = TieredThresholdPromotion::calculate_discounted_price(&tier, &item)?;

        assert_eq!(discounted, Money::from_minor(100, GBP));

        Ok(())
    }
}
