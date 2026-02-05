//! Item Groups

use rusty_money::iso::Currency;
use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    basket::Basket,
    items::Item,
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// Errors related to item group construction or totals.
#[derive(Debug, Error)]
pub enum ItemGroupError {
    /// An item's currency differs from the group currency (index, item currency, group currency).
    #[error("Item {0} has currency {1}, but group has currency {2}")]
    CurrencyMismatch(usize, &'static str, &'static str),

    /// An item was not found in the item group.
    #[error("Item {0} not found")]
    ItemNotFound(usize),
}

/// Item Group
#[derive(Debug)]
pub struct ItemGroup<'a, T: TagCollection = StringTagCollection> {
    items: SmallVec<[Item<'a, T>; 10]>,
    currency: &'a Currency,
}

impl<'a, T: TagCollection> ItemGroup<'a, T> {
    /// Create a new item group with items and currency.
    pub fn new(items: SmallVec<[Item<'a, T>; 10]>, currency: &'a Currency) -> Self {
        ItemGroup { items, currency }
    }

    /// Iterate over the items in the item group.
    pub fn iter(&self) -> impl Iterator<Item = &Item<'_, T>> {
        self.items.iter()
    }

    /// Get an item from the group by its index.
    ///
    /// # Errors
    ///
    /// Returns a `ItemGroupError::ItemNotFound` if the item is not found.
    pub fn get_item(&self, item: usize) -> Result<&Item<'a, T>, ItemGroupError> {
        self.items
            .get(item)
            .ok_or(ItemGroupError::ItemNotFound(item))
    }

    /// Get the currency of the item group.
    pub fn currency(&self) -> &'a Currency {
        self.currency
    }

    /// Get the number of items in the item group.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the item group is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<'a> From<&'a Basket<'a>> for ItemGroup<'a> {
    fn from(basket: &'a Basket<'a>) -> Self {
        ItemGroup {
            items: basket.iter().cloned().collect(),
            currency: basket.currency(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso::GBP};
    use smallvec::SmallVec;
    use testresult::TestResult;

    use crate::{basket::Basket, items::Item, products::ProductKey};

    use super::*;

    fn test_items<'a>() -> [Item<'a>; 2] {
        [
            Item::new(ProductKey::default(), Money::from_minor(100, GBP)),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ]
    }

    #[test]
    fn get_item_returns_item() -> TestResult {
        let items: SmallVec<[Item<'_>; 10]> = test_items().into_iter().collect();
        let group = ItemGroup::new(items, GBP);

        let item = group.get_item(1)?;

        assert_eq!(item.price().to_minor_units(), 200);

        Ok(())
    }

    #[test]
    fn get_item_missing_returns_error() {
        let items: SmallVec<[Item<'_>; 10]> = test_items().into_iter().collect();
        let group = ItemGroup::new(items, GBP);

        let err = group.get_item(99).err();

        assert!(matches!(err, Some(ItemGroupError::ItemNotFound(99))));
    }

    #[test]
    fn from_basket_clones_items_and_currency() -> TestResult {
        let basket = Basket::with_items(test_items(), GBP)?;

        let group = ItemGroup::from(&basket);

        assert_eq!(group.currency(), GBP);
        assert_eq!(group.len(), 2);

        let prices: Vec<i64> = group
            .iter()
            .map(|item| item.price().to_minor_units())
            .collect();

        assert_eq!(prices, vec![100, 200]);

        Ok(())
    }
}
