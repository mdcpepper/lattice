//! Basket

use rusty_money::{Money, iso::Currency};
use thiserror::Error;

use crate::{
    items::Item,
    pricing::{TotalPriceError, total_price},
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// Errors related to basket construction or totals.
#[derive(Debug, Error)]
pub enum BasketError {
    /// An item's currency differs from the basket currency (index, item currency, basket currency).
    #[error("Item {0} has currency {1}, but basket has currency {2}")]
    CurrencyMismatch(usize, &'static str, &'static str),

    /// An item was not found in the basket.
    #[error("Item {0} not found")]
    ItemNotFound(usize),
}

/// Basket
#[derive(Debug)]
pub struct Basket<'a, T: TagCollection = StringTagCollection> {
    items: Vec<Item<'a, T>>,
    currency: &'static Currency,
}

impl<'a, T: TagCollection> Basket<'a, T> {
    /// Create a new basket with the given items.
    #[must_use]
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
        items: impl Into<Vec<Item<'a, T>>>,
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

    /// Get an item from the basket.
    ///
    /// # Errors
    ///
    /// Returns a `BasketError::ItemNotFound` if the item is not found.
    pub fn get_item(&'a self, item: usize) -> Result<&'a Item<'a, T>, BasketError> {
        self.items.get(item).ok_or(BasketError::ItemNotFound(item))
    }

    /// Iterate over the items in the basket.
    pub fn iter(&self) -> impl Iterator<Item = &Item<'_, T>> {
        self.items.iter()
    }

    /// Get the number of items in the basket.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the basket is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the currency of the basket.
    #[must_use]
    pub fn currency(&self) -> &'static Currency {
        self.currency
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{
        Money,
        iso::{GBP, USD},
    };
    use testresult::TestResult;

    use crate::products::ProductKey;

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 3] {
        [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(300, GBP)),
        ]
    }

    #[test]
    fn new_with_currency() {
        let basket = Basket::<'_, StringTagCollection>::new(GBP);

        assert_eq!(basket.currency, GBP);
    }

    #[test]
    fn with_items_currency_mismatch_errors() {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(100, USD)),
        ];

        let result = Basket::<'_, StringTagCollection>::with_items(items, GBP);

        match result {
            Err(BasketError::CurrencyMismatch(idx, item_currency, basket_currency)) => {
                assert_eq!(idx, 1);
                assert_eq!(item_currency, USD.iso_alpha_code);
                assert_eq!(basket_currency, GBP.iso_alpha_code);
            }
            other => panic!("expected CurrencyMismatch error, got {other:?}"),
        }
    }

    #[test]
    fn with_items_all_same_currency_succeeds() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, GBP)?;

        assert_eq!(basket.len(), 3);
        assert_eq!(basket.currency(), GBP);

        Ok(())
    }

    #[test]
    fn subtotal_with_items() -> TestResult {
        let items = [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        let basket = Basket::<'_, StringTagCollection>::with_items(items, GBP)?;

        assert_eq!(basket.subtotal()?, Money::from_minor(300, GBP));

        Ok(())
    }

    #[test]
    fn subtotal_with_no_items() -> TestResult {
        let basket = Basket::<'_, StringTagCollection>::new(GBP);

        assert_eq!(basket.subtotal()?, Money::from_minor(0, GBP));

        Ok(())
    }

    #[test]
    fn len() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, GBP)?;

        assert_eq!(basket.len(), 3);

        Ok(())
    }

    #[test]
    fn is_empty() -> TestResult {
        let empty_basket = Basket::<'_, StringTagCollection>::with_items([], GBP)?;
        let non_empty_basket = Basket::with_items(test_items(), GBP)?;

        assert!(empty_basket.is_empty());
        assert!(!non_empty_basket.is_empty());

        Ok(())
    }

    #[test]
    fn iter_returns_items_in_order() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, GBP)?;

        let prices: Vec<i64> = basket
            .iter()
            .map(|item| item.price().to_minor_units())
            .collect();

        assert_eq!(prices, vec![100, 200, 300]);

        Ok(())
    }

    #[test]
    fn get_item_returns_item() -> TestResult {
        let items = test_items();

        let basket = Basket::with_items(items, GBP)?;
        let item = basket.get_item(1)?;

        assert_eq!(item.price().to_minor_units(), 200);

        Ok(())
    }

    #[test]
    fn get_item_missing_returns_error() {
        let basket = Basket::<'_, StringTagCollection>::new(GBP);

        let err = basket.get_item(0).err();

        assert!(matches!(err, Some(BasketError::ItemNotFound(0))));
    }
}
