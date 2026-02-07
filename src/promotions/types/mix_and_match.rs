//! Mix and Match Bundle Promotion
//!
//! Defines a bundle as a set of slots, each with its own tag eligibility and
//! quantity requirements. Bundles can apply discounts across all items or
//! only to the cheapest item.

use decimal_percentage::Percentage;
use rusty_money::{Money, iso::Currency};

use crate::{
    promotions::{PromotionKey, PromotionSlotKey, budget::PromotionBudget},
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// Discount variants supported by mix-and-match bundles.
#[derive(Debug, Clone)]
pub enum MixAndMatchDiscount<'a> {
    /// Percentage discount applied to all items in the bundle.
    PercentAllItems(Percentage),

    /// Percentage discount applied only to the cheapest item in the bundle.
    PercentCheapest(Percentage),

    /// Fixed total price for the whole bundle.
    FixedTotal(Money<'a, Currency>),

    /// Fixed price applied only to the cheapest item in the bundle.
    FixedCheapest(Money<'a, Currency>),
}

/// Slot definition for a mix-and-match bundle.
#[derive(Debug, Clone)]
pub struct MixAndMatchSlot<T: TagCollection = StringTagCollection> {
    /// Key for a human-readable name for this slot (e.g. "main", "drink", "snack").
    key: PromotionSlotKey,

    /// Tags that match items to this slot (OR semantics).
    tags: T,

    /// Minimum number of items required in this slot.
    min: usize,

    /// Maximum number of items allowed in this slot (None = unlimited).
    max: Option<usize>,
}

impl<T: TagCollection> MixAndMatchSlot<T> {
    /// Create a new slot.
    pub fn new(key: PromotionSlotKey, tags: T, min: usize, max: Option<usize>) -> Self {
        Self {
            key,
            tags,
            min,
            max,
        }
    }

    /// Slot key.
    pub fn key(&self) -> &PromotionSlotKey {
        &self.key
    }

    /// Slot tags.
    pub fn tags(&self) -> &T {
        &self.tags
    }

    /// Minimum required items.
    pub fn min(&self) -> usize {
        self.min
    }

    /// Maximum allowed items.
    pub fn max(&self) -> Option<usize> {
        self.max
    }
}

/// Mix-and-match bundle promotion.
#[derive(Debug, Clone)]
pub struct MixAndMatchPromotion<'a, T: TagCollection = StringTagCollection> {
    key: PromotionKey,
    slots: Vec<MixAndMatchSlot<T>>,
    discount: MixAndMatchDiscount<'a>,
    budget: PromotionBudget<'a>,
}

impl<'a, T: TagCollection> MixAndMatchPromotion<'a, T> {
    /// Create a new mix-and-match promotion.
    #[must_use]
    pub fn new(
        key: PromotionKey,
        slots: Vec<MixAndMatchSlot<T>>,
        discount: MixAndMatchDiscount<'a>,
        budget: PromotionBudget<'a>,
    ) -> Self {
        Self {
            key,
            slots,
            discount,
            budget,
        }
    }

    /// Promotion key.
    #[must_use]
    pub fn key(&self) -> PromotionKey {
        self.key
    }

    /// Slots.
    #[must_use]
    pub fn slots(&self) -> &[MixAndMatchSlot<T>] {
        &self.slots
    }

    /// Discount.
    #[must_use]
    pub fn discount(&self) -> &MixAndMatchDiscount<'a> {
        &self.discount
    }

    /// Return the budget
    #[must_use]
    pub const fn budget(&self) -> &PromotionBudget<'a> {
        &self.budget
    }

    /// True if all slots have fixed arity (min == max).
    #[must_use]
    pub fn has_fixed_arity(&self) -> bool {
        self.slots
            .iter()
            .all(|slot| slot.max.is_some_and(|max| max == slot.min))
    }

    /// Total bundle size when all slots have fixed arity.
    #[must_use]
    pub fn bundle_size(&self) -> usize {
        self.slots.iter().map(|slot| slot.min).sum()
    }
}

#[cfg(test)]
mod tests {
    use decimal_percentage::Percentage;
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;

    use crate::{tags::string::StringTagCollection, utils::slot};

    use super::*;

    #[test]
    fn accessors_return_constructor_values() {
        let key = PromotionKey::default();
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
        let slots = vec![
            slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["main"]),
                1,
                Some(1),
            ),
            slot(
                &mut slot_keys,
                StringTagCollection::from_strs(&["drink"]),
                1,
                Some(1),
            ),
        ];
        let discount = MixAndMatchDiscount::PercentAllItems(Percentage::from(0.25));

        let promo =
            MixAndMatchPromotion::new(key, slots.clone(), discount, PromotionBudget::unlimited());

        assert_eq!(promo.key(), key);
        assert_eq!(promo.slots().len(), 2);
        assert!(matches!(
            promo.discount(),
            MixAndMatchDiscount::PercentAllItems(_)
        ));
        assert!(promo.has_fixed_arity());
        assert_eq!(promo.bundle_size(), 2);

        let _ = MixAndMatchDiscount::FixedTotal(Money::from_minor(500, GBP));
    }

    #[test]
    fn slot_accessors_return_expected_values() {
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
        let slot_key = slot_keys.insert(());
        let slot = MixAndMatchSlot::new(
            slot_key,
            StringTagCollection::from_strs(&["tag1", "tag2"]),
            3,
            Some(5),
        );

        assert_eq!(slot.key(), &slot_key);
        let tags = slot.tags().to_strs();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().any(|t| t == "tag1"));
        assert!(tags.iter().any(|t| t == "tag2"));
        assert_eq!(slot.min(), 3);
        assert_eq!(slot.max(), Some(5));
    }
}
