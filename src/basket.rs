//! Basket

use rusty_money::{Money, iso::Currency};
use thiserror::Error;

use crate::{
    items::Item,
    pricing::{TotalPriceError, total_price},
};

/// Errors related to basket construction or totals.
#[derive(Debug, Error)]
pub enum BasketError {
    /// An item's currency differs from the basket currency (index, item currency, basket currency).
    #[error("Item {0} has currency {1}, but basket has currency {2}")]
    CurrencyMismatch(usize, &'static str, &'static str),
}

/// Basket
#[derive(Debug)]
pub struct Basket<'a> {
    items: Vec<Item<'a>>,
    currency: &'static Currency,
}

impl<'a> Basket<'a> {
    /// Create a new basket with the given items.
    pub fn new(currency: &'static Currency) -> Self {
        Basket {
            items: Vec::new(),
            currency,
        }
    }

    /// Create a new basket with the given items.
    ///
    /// # Errors
    ///
    /// Returns a `BasketError` if there was a currency mismatch error.
    pub fn with_items(
        items: impl Into<Vec<Item<'a>>>,
        currency: &'static Currency,
    ) -> Result<Self, BasketError> {
        let items = items.into();

        items.iter().enumerate().try_for_each(|(i, item)| {
            let item_currency = item.price().currency();
            if item_currency == currency {
                Ok(())
            } else {
                Err(BasketError::CurrencyMismatch(
                    i,
                    item_currency.iso_alpha_code,
                    currency.iso_alpha_code,
                ))
            }
        })?;

        Ok(Basket { items, currency })
    }

    /// Calculate the subtotal of the basket.
    ///
    /// # Errors
    ///
    /// Returns a `TotalPriceError` if there was a money arithmetic or currency mismatch error.
    pub fn subtotal(&'a self) -> Result<Money<'a, Currency>, TotalPriceError> {
        if self.is_empty() {
            return Ok(Money::from_minor(0, self.currency));
        }

        total_price(&self.items)
    }

    /// Get the number of items in the basket.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the basket is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the currency of the basket.
    pub fn currency(&self) -> &'static Currency {
        self.currency
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use testresult::TestResult;

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(Money::from_minor(100, iso::GBP)),
            Item::new(Money::from_minor(200, iso::GBP)),
            Item::new(Money::from_minor(300, iso::GBP)),
        ]
    }

    #[test]
    fn new_with_currency() {
        let basket = Basket::new(iso::GBP);

        assert_eq!(basket.currency, iso::GBP);
    }

    #[test]
    fn with_items_currency_mismatch_errors() {
        let items = [
            Item::new(Money::from_minor(100, iso::GBP)),
            Item::new(Money::from_minor(100, iso::USD)),
        ];

        let result = Basket::with_items(items, iso::GBP);

        match result {
            Err(BasketError::CurrencyMismatch(idx, item_currency, basket_currency)) => {
                assert_eq!(idx, 1);
                assert_eq!(item_currency, iso::USD.iso_alpha_code);
                assert_eq!(basket_currency, iso::GBP.iso_alpha_code);
            }
            other => panic!("expected CurrencyMismatch error, got {other:?}"),
        }
    }

    #[test]
    fn with_items_all_same_currency_succeeds() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, iso::GBP)?;

        assert_eq!(basket.len(), 3);
        assert_eq!(basket.currency(), iso::GBP);

        Ok(())
    }

    #[test]
    fn subtotal_with_items() -> TestResult {
        let items = [
            Item::new(Money::from_minor(100, iso::GBP)),
            Item::new(Money::from_minor(200, iso::GBP)),
        ];

        let basket = Basket::with_items(items, iso::GBP)?;

        assert_eq!(basket.subtotal()?, Money::from_minor(300, iso::GBP));

        Ok(())
    }

    #[test]
    fn subtotal_with_no_items() -> TestResult {
        let basket = Basket::new(iso::GBP);

        assert_eq!(basket.subtotal()?, Money::from_minor(0, iso::GBP));

        Ok(())
    }

    #[test]
    fn len() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, iso::GBP)?;

        assert_eq!(basket.len(), 3);

        Ok(())
    }

    #[test]
    fn is_empty() -> TestResult {
        let empty_basket = Basket::with_items([], iso::GBP)?;
        let non_empty_basket = Basket::with_items(test_items(), iso::GBP)?;

        assert!(empty_basket.is_empty());
        assert!(!non_empty_basket.is_empty());

        Ok(())
    }
}
