//! Promotion Applications

use decimal_percentage::Percentage;
use num_traits::FromPrimitive;
use rust_decimal::Decimal;
use rusty_money::{Money, MoneyError, iso::Currency};

use crate::promotions::PromotionKey;

/// Result of applying a promotion to an item
#[derive(Debug, Clone)]
pub struct PromotionApplication<'a> {
    /// Key of the promotion that was applied
    pub promotion_key: PromotionKey,

    /// Index of the item in the item group
    pub item_idx: usize,

    /// ID assigned to a bundle of items in the same promotion
    pub bundle_id: usize,

    /// Original price of the item
    pub original_price: Money<'a, Currency>,

    /// Final price after discount
    pub final_price: Money<'a, Currency>,
}

impl<'a> PromotionApplication<'_> {
    /// Calculate the item savings from this promotion application
    ///
    /// # Errors
    ///
    /// Returns an error if the original price or final price cannot be subtracted.
    pub fn savings(&'a self) -> Result<Money<'a, Currency>, MoneyError> {
        self.original_price.sub(self.final_price)
    }

    /// Calculates the savings made by applying the promotions as a percentage
    ///
    /// # Errors
    ///
    /// Returns a [`MoneyError`] if the subtraction operation fails.
    pub fn savings_percent(&self) -> Result<Percentage, MoneyError> {
        let savings = self.savings()?;

        // Percent savings is relative to the original (pre-discount) subtotal.
        // Avoid integer division truncation by doing the ratio in decimal space.
        let savings_minor = savings.to_minor_units();
        let subtotal_minor = self.original_price.to_minor_units();

        if subtotal_minor == 0 {
            return Ok(Percentage::from(0.0));
        }

        let savings_dec = Decimal::from_i64(savings_minor).unwrap_or(Decimal::ZERO);
        let subtotal_dec = Decimal::from_i64(subtotal_minor).unwrap_or(Decimal::ZERO);

        Ok(Percentage::from(savings_dec / subtotal_dec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use rusty_money::iso::{GBP, USD};

    #[test]
    fn savings_returns_difference_between_original_and_final() {
        let app = PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 0,
            original_price: Money::from_minor(200, GBP),
            final_price: Money::from_minor(150, GBP),
        };

        assert_eq!(app.savings(), Ok(Money::from_minor(50, GBP)));
    }

    #[test]
    fn savings_errors_on_currency_mismatch() {
        let app = PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 0,
            original_price: Money::from_minor(200, USD),
            final_price: Money::from_minor(150, GBP),
        };

        assert_eq!(
            app.savings(),
            Err(MoneyError::CurrencyMismatch {
                expected: USD.iso_alpha_code,
                actual: GBP.iso_alpha_code,
            })
        );
    }

    #[test]
    fn savings_percent_is_zero_when_original_price_is_zero() {
        let app = PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 0,
            original_price: Money::from_minor(0, GBP),
            final_price: Money::from_minor(0, GBP),
        };

        assert_eq!(app.savings_percent(), Ok(Percentage::from(0.0)));
    }

    #[test]
    fn savings_percent_is_correct_for_nonzero_original_price() -> Result<(), MoneyError> {
        let app = PromotionApplication {
            promotion_key: PromotionKey::default(),
            item_idx: 0,
            bundle_id: 0,
            original_price: Money::from_minor(200, GBP),
            final_price: Money::from_minor(150, GBP),
        };

        let percent = app.savings_percent()?;
        let percent_points = ((percent * Decimal::ONE)
            * Decimal::from_i64(100).expect("Failed to convert to Decimal"))
        .round_dp(2);

        assert_eq!(
            percent_points,
            Decimal::from_i64(25).expect("Failed to convert to Decimal")
        );
        Ok(())
    }
}
