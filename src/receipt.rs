//! Receipt

use rusty_money::{Money, MoneyError, iso::Currency};
use smallvec::SmallVec;

/// Final receipt for a processed basket.
#[derive(Debug, Clone)]
pub struct Receipt<'a> {
    /// Indexes of items in the basket that were purchased at full price, not in any promotion
    _full_price_items: SmallVec<[usize; 10]>,

    /// Total cost before any promotion applications
    subtotal: Money<'a, Currency>,

    /// Total amount paid for all items after any promotion applications
    total: Money<'a, Currency>,

    /// Currency used for all monetary values
    _currency: &'static Currency,
}

impl<'a> Receipt<'a> {
    /// Create a new receipt with the given details.
    pub fn new(
        full_price_items: SmallVec<[usize; 10]>,
        subtotal: Money<'a, Currency>,
        total: Money<'a, Currency>,
        currency: &'static Currency,
    ) -> Self {
        Self {
            _full_price_items: full_price_items,
            subtotal,
            total,
            _currency: currency,
        }
    }

    /// Total cost before any promotion applications
    pub fn subtotal(&self) -> Money<'a, Currency> {
        self.subtotal
    }

    /// Total amount paid for all items
    pub fn total(&self) -> Money<'a, Currency> {
        self.total
    }

    /// Calculate the savings made by applying promotions.
    ///
    /// # Errors
    ///
    /// Returns a [`MoneyError`] if the subtraction operation fails.
    pub fn savings(&self) -> Result<Money<'a, Currency>, MoneyError> {
        self.subtotal.sub(self.total)
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use smallvec::smallvec;
    use testresult::TestResult;

    use super::*;

    #[test]
    fn accessors_return_values_from_constructor() {
        let receipt = Receipt::new(
            smallvec![0, 2],
            Money::from_minor(300, iso::GBP),
            Money::from_minor(250, iso::GBP),
            iso::GBP,
        );

        assert_eq!(receipt.subtotal(), Money::from_minor(300, iso::GBP));
        assert_eq!(receipt.total(), Money::from_minor(250, iso::GBP));
    }

    #[test]
    fn savings_is_subtotal_minus_total() -> TestResult {
        let receipt = Receipt::new(
            smallvec![0, 1],
            Money::from_minor(300, iso::GBP),
            Money::from_minor(250, iso::GBP),
            iso::GBP,
        );

        assert_eq!(receipt.savings()?, Money::from_minor(50, iso::GBP));
        Ok(())
    }

    #[test]
    fn savings_errors_on_currency_mismatch() {
        let receipt = Receipt::new(
            smallvec![0],
            Money::from_minor(300, iso::GBP),
            Money::from_minor(250, iso::USD),
            iso::GBP,
        );

        assert_eq!(
            receipt.savings(),
            Err(MoneyError::CurrencyMismatch {
                expected: iso::GBP.iso_alpha_code,
                actual: iso::USD.iso_alpha_code,
            })
        );
    }
}
