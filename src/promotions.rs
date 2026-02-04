//! Promotions

use slotmap::new_key_type;

use crate::{
    items::groups::ItemGroup, promotions::direct_discount::DirectDiscountPromotion,
    solvers::ilp::promotions::ILPPromotion,
};

pub mod applications;
pub mod direct_discount;

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
    /// Direct Discount Promotion
    DirectDiscount(DirectDiscountPromotion<'a>),
}

impl Promotion<'_> {
    /// Return the promotion key.
    pub fn key(&self) -> PromotionKey {
        match self {
            Promotion::DirectDiscount(direct_discount) => direct_discount.key(),
        }
    }

    /// Return whether this promotion _might_ apply to the given item group.
    pub fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        match self {
            Promotion::DirectDiscount(direct_discount) => direct_discount.is_applicable(item_group),
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use slotmap::SlotMap;
    use smallvec::SmallVec;

    use crate::{
        items::groups::ItemGroup,
        promotions::direct_discount::{DirectDiscount, DirectDiscountPromotion},
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    #[test]
    fn key_delegates_to_inner_promotion_key() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), iso::GBP);

        // Generate a non-default promotion key so returning `Default::default()` is detectable.
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let inner = DirectDiscountPromotion::new(
            key,
            StringTagCollection::empty(),
            DirectDiscount::AmountOverride(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::DirectDiscount(inner);

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

        let inner = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            DirectDiscount::AmountOverride(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::DirectDiscount(inner);

        assert!(!promo.is_applicable(&item_group));
    }
}
