//! Tiered Threshold Promotion
//!
//! A promotion that checks whether items matching `contribution_tags` meet a spend threshold,
//! then applies a [`ThresholdDiscount`] to items matching `discount_tags`. Multiple tiers can be
//! defined (e.g., spend £50 for 5% off, spend £80 for 12% off); the ILP solver selects the
//! single best tier that minimises total basket cost.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::Currency};

use crate::{
    discounts::{DiscountError, percent_of_minor},
    items::Item,
    promotions::{PromotionKey, budget::PromotionBudget},
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

/// A single threshold tier within a tiered threshold promotion.
///
/// Each tier specifies a spend threshold, which items contribute to that threshold,
/// which items receive the discount, and what discount is applied.
#[derive(Debug, Clone)]
pub struct ThresholdTier<'a, T: TagCollection = StringTagCollection> {
    threshold: Money<'a, Currency>,
    contribution_tags: T,
    discount_tags: T,
    discount: ThresholdDiscount<'a>,
}

impl<'a, T: TagCollection> ThresholdTier<'a, T> {
    /// Create a new threshold tier.
    pub fn new(
        threshold: Money<'a, Currency>,
        contribution_tags: T,
        discount_tags: T,
        discount: ThresholdDiscount<'a>,
    ) -> Self {
        Self {
            threshold,
            contribution_tags,
            discount_tags,
            discount,
        }
    }

    /// Return the spend threshold.
    pub fn threshold(&self) -> &Money<'a, Currency> {
        &self.threshold
    }

    /// Return the contribution tags.
    pub fn contribution_tags(&self) -> &T {
        &self.contribution_tags
    }

    /// Return the discount tags.
    pub fn discount_tags(&self) -> &T {
        &self.discount_tags
    }

    /// Return the discount.
    pub fn discount(&self) -> &ThresholdDiscount<'a> {
        &self.discount
    }
}

/// A tiered threshold promotion.
///
/// Evaluates items against spend thresholds: if items matching `contribution_tags` meet
/// a tier's threshold, items matching `discount_tags` receive the tier's discount. The ILP
/// solver picks the single best qualifying tier to minimise total cost.
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
            Money::from_minor(threshold_minor, GBP),
            StringTagCollection::from_strs(&["wine"]),
            StringTagCollection::from_strs(&["cheese"]),
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
                .map(|t| t.threshold().to_minor_units()),
            Some(5000)
        );
        assert!(
            promo
                .tiers()
                .first()
                .is_some_and(|t| t.contribution_tags().contains("wine"))
        );
        assert!(
            promo
                .tiers()
                .first()
                .is_some_and(|t| t.discount_tags().contains("cheese"))
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
