//! Direct Discount
//!
//! A direct percentage discount, amount discount, or amount override on all qualifying items

use crate::{
    discounts::{DiscountError, SimpleDiscount, percent_of_minor},
    items::Item,
    promotions::{PromotionKey, budget::PromotionBudget, qualification::Qualification},
    tags::{collection::TagCollection, string::StringTagCollection},
};
use rusty_money::{Money, iso::Currency};

/// A discount applied directly to all participating items
#[derive(Debug, Clone)]
pub struct DirectDiscountPromotion<'a, T: TagCollection = StringTagCollection> {
    key: PromotionKey,
    qualification: Qualification<T>,
    discount: SimpleDiscount<'a>,
    budget: PromotionBudget<'a>,
}

impl<'a, T: TagCollection> DirectDiscountPromotion<'a, T> {
    /// Create a new direct discount promotion.
    pub fn new(
        key: PromotionKey,
        qualification: Qualification<T>,
        discount: SimpleDiscount<'a>,
        budget: PromotionBudget<'a>,
    ) -> Self {
        Self {
            key,
            qualification,
            discount,
            budget,
        }
    }

    /// Return the promotion key
    pub fn key(&self) -> PromotionKey {
        self.key
    }

    /// Return the item qualification expression.
    pub fn qualification(&self) -> &Qualification<T> {
        &self.qualification
    }

    /// Return the discount
    pub fn discount(&self) -> &SimpleDiscount<'a> {
        &self.discount
    }

    /// Return the budget
    pub const fn budget(&self) -> &PromotionBudget<'a> {
        &self.budget
    }

    /// Calculate the discounted price for a single item.
    ///
    /// # Errors
    ///
    /// Returns a [`DiscountError`] if:
    /// - Percentage calculation overflows or cannot be safely represented.
    /// - Money arithmetic fails (e.g., currency mismatch, negative result).
    pub fn calculate_discounted_price(
        &self,
        item: &Item<'a, T>,
    ) -> Result<Money<'a, Currency>, DiscountError> {
        let discounted_minor = match &self.discount {
            SimpleDiscount::PercentageOff(pct) => {
                // Calculate the discount amount in minor units
                let original_minor = item.price().to_minor_units();

                original_minor
                    .checked_sub(percent_of_minor(pct, original_minor)?)
                    .ok_or(DiscountError::PercentConversion)?
            }
            SimpleDiscount::AmountOverride(amount) => {
                // Replace price with fixed amount
                amount.to_minor_units()
            }
            SimpleDiscount::AmountOff(amount) => {
                // Subtract amount from price
                item.price().sub(*amount)?.to_minor_units()
            }
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

    use crate::{items::Item, products::ProductKey, promotions::qualification::Qualification};

    use super::*;

    #[test]
    fn key_returns_constructor_key() {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let promo = DirectDiscountPromotion::new(
            key,
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(0, GBP)),
            PromotionBudget::unlimited(),
        );

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());
    }

    #[test]
    fn calculate_discounted_price_percentage() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(75, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_amount_override() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(50, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_amount_discount_off() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::AmountOff(Money::from_minor(25, GBP)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(75, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_percentage_to_zero() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::PercentageOff(Percentage::from(2.0)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_amount_off_to_zero() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::AmountOff(Money::from_minor(200, GBP)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn calculate_discounted_price_clamps_amount_override_to_zero() -> TestResult {
        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            Qualification::<StringTagCollection>::match_all(),
            SimpleDiscount::AmountOverride(Money::from_minor(-50, GBP)),
            PromotionBudget::unlimited(),
        );

        let item = Item::new(ProductKey::default(), Money::from_minor(100, GBP));
        let discounted = promo.calculate_discounted_price(&item)?;

        assert_eq!(discounted, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn accessors_return_constructor_values() {
        let qualification =
            Qualification::match_any(StringTagCollection::from_strs(&["member", "sale"]));
        let discount = SimpleDiscount::AmountOff(Money::from_minor(10, GBP));

        let promo = DirectDiscountPromotion::new(
            PromotionKey::default(),
            qualification.clone(),
            discount,
            PromotionBudget::unlimited(),
        );

        assert!(
            promo
                .qualification()
                .matches(&StringTagCollection::from_strs(&["member"]))
        );
        assert_eq!(promo.qualification().rules.len(), qualification.rules.len());
        assert!(matches!(
            promo.discount(),
            SimpleDiscount::AmountOff(amount) if amount.to_minor_units() == 10
        ));
    }
}
