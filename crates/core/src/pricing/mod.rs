//! Prices

use rusty_money::{Money, MoneyError, iso::Currency};
use thiserror::Error;

use crate::{items::Item, tags::collection::TagCollection};

/// Errors that can occur while calculating total price.
#[derive(Debug, Error, PartialEq)]
pub enum TotalPriceError {
    /// No items were provided, so currency could not be determined.
    #[error("no items provided; cannot determine currency")]
    NoItems,

    /// Wrapped money arithmetic or currency mismatch error.
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// Calculates the total price of a list of items
///
/// # Errors
///
/// - [`TotalPriceError::NoItems`]: No items were provided, so currency could not be determined.
/// - [`TotalPriceError::Money`]: Wrapped money arithmetic or currency mismatch error.
pub fn total_price<'a, T: TagCollection>(
    items: &[Item<'a, T>],
) -> Result<Money<'a, Currency>, TotalPriceError> {
    let first = items.first().ok_or(TotalPriceError::NoItems)?;

    let total = items.iter().try_fold(
        Money::from_minor(0, first.price().currency()),
        |acc, item| acc.add(*item.price()),
    )?;

    Ok(total)
}

#[cfg(test)]
mod tests {
    use rusty_money::iso::GBP;
    use testresult::TestResult;

    use crate::{products::ProductKey, tags::string::StringTagCollection};

    use super::*;

    #[test]
    fn test_total_price() -> TestResult {
        let items: [Item<'_, StringTagCollection>; 2] = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        assert_eq!(total_price(&items)?, Money::from_minor(300, GBP));

        Ok(())
    }

    #[test]
    fn test_total_price_empty() {
        let items: [Item<'static>; 0] = [];

        assert!(matches!(total_price(&items), Err(TotalPriceError::NoItems)));
    }
}
