//! Items

use rusty_money::{Money, iso};

/// An unprocessed item
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Item<'a> {
    price: Money<'a, iso::Currency>,
}

impl<'a> Item<'a> {
    /// Creates a new item with the given price
    pub fn new(price: Money<'a, iso::Currency>) -> Self {
        Self { price }
    }

    /// Returns the price of the item
    pub fn price(&self) -> &Money<'a, iso::Currency> {
        &self.price
    }
}

/// Returns the cheapest item in a list of items
pub fn cheapest_item<'a>(items: &'a [Item<'a>]) -> Option<&'a Item<'a>> {
    items
        .iter()
        .min_by_key(|item| item.price().to_minor_units())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cheapest_item() {
        let item_1 = Item::new(Money::from_minor(100, iso::USD));
        let item_2 = Item::new(Money::from_minor(200, iso::USD));
        let items = [item_1, item_2];

        assert_eq!(cheapest_item(&items), Some(&item_1));
    }
}
