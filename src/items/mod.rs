//! Items

use rusty_money::{Money, iso::Currency};

use crate::{
    products::ProductKey,
    tags::{collection::TagCollection, string::StringTagCollection},
};

pub mod groups;

/// An unprocessed item with a price and tags.
#[derive(Clone, Debug, PartialEq)]
pub struct Item<'a, T: TagCollection = StringTagCollection> {
    product: ProductKey,
    price: Money<'a, Currency>,
    tags: T,
}

impl<'a, T: TagCollection> Item<'a, T> {
    /// Creates a new item with the given price and empty tags.
    #[must_use]
    pub fn new(product: ProductKey, price: Money<'a, Currency>) -> Self {
        Self::with_tags(product, price, T::empty())
    }

    /// Creates a new item with the given price and tags.
    pub fn with_tags(product: ProductKey, price: Money<'a, Currency>, tags: T) -> Self {
        Self {
            product,
            price,
            tags,
        }
    }

    /// Returns the product of the item
    pub fn product(&self) -> ProductKey {
        self.product
    }

    /// Returns the price of the item
    pub fn price(&self) -> &Money<'a, Currency> {
        &self.price
    }

    /// Returns the tags for the item.
    pub fn tags(&self) -> &T {
        &self.tags
    }

    /// Returns the tags for the item, mutably.
    pub fn tags_mut(&mut self) -> &mut T {
        &mut self.tags
    }
}

/// Returns the cheapest item in a list of items
pub fn cheapest_item<'a, T: TagCollection>(items: &'a [Item<'a, T>]) -> Option<&'a Item<'a, T>> {
    items
        .iter()
        .min_by_key(|item| item.price().to_minor_units())
}

#[cfg(test)]
mod tests {
    use rusty_money::iso::GBP;

    use super::*;

    #[test]
    fn test_cheapest_item() {
        let items: [Item<'_, StringTagCollection>; 2] = [
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::empty(),
            ),
            Item::new(ProductKey::default(), Money::from_minor(200, GBP)),
        ];

        let cheapest = cheapest_item(&items).expect("expected cheapest item");
        assert_eq!(cheapest.price(), &Money::from_minor(100, GBP));
    }

    #[test]
    fn item_tag_accessors_work() {
        let tags = StringTagCollection::from_strs(&["fresh"]);
        let mut item = Item::with_tags(ProductKey::default(), Money::from_minor(150, GBP), tags);

        assert!(item.tags().contains("fresh"));

        item.tags_mut().add("sale");
        assert!(item.tags().contains("sale"));
    }

    #[test]
    fn item_product_accessor_returns_key() {
        let key = ProductKey::default();
        let item: Item<'_, StringTagCollection> = Item::new(key, Money::from_minor(100, GBP));

        assert_eq!(item.product(), key);
    }
}
