//! Simple Discount
//!
//! A simple fixed amount or percentage discount on all qualifying items

use crate::{
    discounts::Discount,
    promotions::PromotionKey,
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// A Simple Fixed or Percentage Discount
#[derive(Debug, Copy, Clone)]
pub struct SimpleDiscount<'a, T: TagCollection = StringTagCollection> {
    key: PromotionKey,
    tags: T,
    discount: Discount<'a>,
}

impl<'a, T: TagCollection> SimpleDiscount<'a, T> {
    /// Create a new simple discount promotion.
    pub fn new(key: PromotionKey, tags: T, discount: Discount<'a>) -> Self {
        Self {
            key,
            tags,
            discount,
        }
    }

    /// Return the promotion key
    pub fn key(&self) -> PromotionKey {
        self.key
    }

    /// Return the tags
    pub fn tags(&self) -> &T {
        &self.tags
    }

    /// Returns the discount
    pub fn discount(&self) -> &Discount<'a> {
        &self.discount
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use slotmap::SlotMap;

    use super::*;

    #[test]
    fn key_returns_constructor_key() {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let promo = SimpleDiscount::new(
            key,
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(0, iso::GBP)),
        );

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());
    }
}
