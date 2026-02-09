//! Discount utilities
//!
//! This module provides utility functions for discount calculations
//! that can be shared across different promotion types.

use decimal_percentage::Percentage;
use rust_decimal::{
    Decimal, RoundingStrategy,
    prelude::{FromPrimitive, ToPrimitive},
};
use rusty_money::{Money, MoneyError, iso::Currency};
use thiserror::Error;

/// Errors specific to discount calculations.
#[derive(Debug, Error)]
pub enum DiscountError {
    /// No items provided, so currency cannot be determined.
    #[error("no items provided; cannot determine currency for discount")]
    NoItems,

    /// Percentage calculation could not be safely converted.
    #[error("percentage conversion overflowed or was not finite")]
    PercentConversion,

    /// Wrapped money arithmetic or currency mismatch error.
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// Discount configuration for promotions without complex requirements.
#[derive(Debug, Copy, Clone)]
pub enum SimpleDiscount<'a> {
    /// Apply a percentage discount (e.g., "25% off")
    PercentageOff(Percentage),

    /// Replace item price with a fixed amount (e.g., "£5 each")
    AmountOverride(Money<'a, Currency>),

    /// Subtract a fixed amount from item price (e.g., "£2 off")
    AmountOff(Money<'a, Currency>),
}

/// Calculate the discount amount in minor units based on a percentage and a minor unit amount.
///
/// This is a utility function that can be used by promotion types when calculating
/// percentage-based discounts.
///
/// # Errors
///
/// Returns an error if:
/// - The percentage calculation overflows or cannot be safely represented (`DiscountError::PercentConversion`).
pub fn percent_of_minor(percent: &Percentage, minor: i64) -> Result<i64, DiscountError> {
    let minor = Decimal::from_i64(minor).ok_or(DiscountError::PercentConversion)?;

    ((*percent) * Decimal::ONE) // decimal_percentage crate doesn't actually expose the underlying Decimal
        .checked_mul(minor)
        .ok_or(DiscountError::PercentConversion)?
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i64()
        .ok_or(DiscountError::PercentConversion)
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use decimal_percentage::Percentage;
    use testresult::TestResult;

    use super::*;

    #[test]
    fn percent_of_minor_overflow_returns_error() {
        let percent = Percentage::from(2.0);
        let result = percent_of_minor(&percent, i64::MAX);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn percent_of_minor_checked_mul_overflow_returns_error() -> TestResult {
        // 1e20 is representable as a Decimal, but multiplying by a very large minor value should
        // overflow the Decimal range.
        let percent = Percentage::try_from("100000000000000000000")?;
        let result = percent_of_minor(&percent, i64::MAX);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));

        Ok(())
    }

    #[test]
    fn percent_of_minor_underflow_returns_error() {
        let percent = Percentage::from(2.0);
        let result = percent_of_minor(&percent, i64::MIN);

        assert!(matches!(result, Err(DiscountError::PercentConversion)));
    }

    #[test]
    fn percent_of_minor_calculates_correctly() -> TestResult {
        let percent = Percentage::from(0.25);
        let result = percent_of_minor(&percent, 200)?;

        assert_eq!(result, 50);

        Ok(())
    }
}
