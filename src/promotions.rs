//! Promotions

use slotmap::new_key_type;

use crate::{
    items::groups::ItemGroup, promotions::simple_discount::SimpleDiscount,
    solvers::ilp::promotions::ILPPromotion,
};

pub mod applications;
pub mod simple_discount;

new_key_type! {
    /// Promotion Key
    pub struct PromotionKey;
}

/// Promotion metadata
#[derive(Debug)]
pub struct PromotionMeta {
    /// Promotion name
    pub name: String,
}

/// Promotion enum
#[derive(Debug, Clone)]
pub enum Promotion<'a> {
    /// Simple discount promotion
    SimpleDiscount(SimpleDiscount<'a>),
}

impl Promotion<'_> {
    /// Return the promotion key.
    pub fn key(&self) -> PromotionKey {
        match self {
            Promotion::SimpleDiscount(simple_discount) => simple_discount.key(),
        }
    }

    /// Return whether this promotion *might* apply to the given item group.
    pub fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        match self {
            Promotion::SimpleDiscount(simple_disount) => simple_disount.is_applicable(item_group),
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use slotmap::SlotMap;
    use smallvec::SmallVec;

    use crate::{
        discounts::Discount,
        items::groups::ItemGroup,
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::{Promotion, PromotionKey, simple_discount::SimpleDiscount};

    #[test]
    fn key_delegates_to_inner_promotion_key() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), iso::GBP);

        // Generate a non-default promotion key so returning `Default::default()` is detectable.
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let inner = SimpleDiscount::new(
            key,
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::SimpleDiscount(inner);

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());

        // Also smoke that this promo is "well-formed" for other calls.
        let _ = promo.is_applicable(&item_group);
    }

    #[test]
    fn is_applicable_delegates_to_inner_promotion() {
        // An empty item set should not be considered applicable; this ensures
        // `Promotion::is_applicable` doesn't accidentally short-circuit to `true`.
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), iso::GBP);

        let inner = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::SimpleDiscount(inner);

        assert!(!promo.is_applicable(&item_group));
    }
}
