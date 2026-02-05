//! Promotions

use slotmap::{SecondaryMap, new_key_type};

use crate::{
    items::groups::ItemGroup,
    promotions::{
        direct_discount::DirectDiscountPromotion, mix_and_match::MixAndMatchPromotion,
        positional_discount::PositionalDiscountPromotion,
    },
    solvers::ilp::promotions::ILPPromotion,
};

pub mod applications;
pub mod direct_discount;
pub mod mix_and_match;
pub mod positional_discount;

new_key_type! {
    /// Promotion Key
    pub struct PromotionKey;
}

new_key_type! {
    /// Promotion Slot Key
    pub struct PromotionSlotKey;
}

/// Promotion metadata
#[derive(Debug, Default)]
pub struct PromotionMeta {
    /// Promotion name
    pub name: String,

    /// Slot names
    pub slot_names: SecondaryMap<PromotionSlotKey, String>,
}

/// Promotion enum
#[derive(Debug, Clone)]
pub enum Promotion<'a> {
    /// Direct Discount Promotion
    DirectDiscount(DirectDiscountPromotion<'a>),

    /// Mix-and-Match Bundle Promotion
    MixAndMatch(MixAndMatchPromotion<'a>),

    /// Positional Discount
    PositionalDiscount(PositionalDiscountPromotion<'a>),
}

impl Promotion<'_> {
    /// Return the promotion key.
    pub fn key(&self) -> PromotionKey {
        match self {
            Promotion::DirectDiscount(direct_discount) => direct_discount.key(),
            Promotion::MixAndMatch(mix_and_match) => mix_and_match.key(),
            Promotion::PositionalDiscount(positional_discount) => positional_discount.key(),
        }
    }

    /// Return whether this promotion _might_ apply to the given item group.
    pub fn is_applicable(&self, item_group: &ItemGroup<'_>) -> bool {
        match self {
            Promotion::DirectDiscount(direct_discount) => direct_discount.is_applicable(item_group),
            Promotion::MixAndMatch(mix_and_match) => mix_and_match.is_applicable(item_group),
            Promotion::PositionalDiscount(positional_discount) => {
                positional_discount.is_applicable(item_group)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;
    use smallvec::SmallVec;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{
            direct_discount::DirectDiscountPromotion,
            mix_and_match::{MixAndMatchPromotion, MixAndMatchSlot},
            positional_discount::PositionalDiscountPromotion,
        },
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::*;

    fn slot(
        keys: &mut SlotMap<PromotionSlotKey, ()>,
        tags: StringTagCollection,
        min: usize,
        max: Option<usize>,
    ) -> MixAndMatchSlot {
        MixAndMatchSlot::new(keys.insert(()), tags, min, max)
    }

    #[test]
    fn key_delegates_to_inner_promotion_key() {
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        // Generate a non-default promotion key so returning `Default::default()` is detectable.
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let inner = DirectDiscountPromotion::new(
            key,
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
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
        let item_group: ItemGroup<'_> = ItemGroup::new(SmallVec::new(), GBP);

        let inner = DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
        );

        let promo = Promotion::DirectDiscount(inner);

        assert!(!promo.is_applicable(&item_group));
    }

    #[test]
    fn key_delegates_to_positional_promotion() {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let inner = PositionalDiscountPromotion::new(
            key,
            StringTagCollection::empty(),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::AmountOff(Money::from_minor(50, GBP)),
        );

        let promo = Promotion::PositionalDiscount(inner);

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());
    }

    #[test]
    fn is_applicable_handles_positional_discount_tags() {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["fresh"]),
        )]);

        let item_group: ItemGroup<'_> = ItemGroup::new(items, GBP);

        let inner = PositionalDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["fresh"]),
            2,
            SmallVec::from_vec(vec![1u16]),
            SimpleDiscount::AmountOff(Money::from_minor(10, GBP)),
        );

        let promo = Promotion::PositionalDiscount(inner);

        assert!(promo.is_applicable(&item_group));
    }

    #[test]
    fn key_delegates_to_mix_and_match_promotion() {
        let mut keys = SlotMap::<PromotionKey, ()>::with_key();
        let key = keys.insert(());

        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();
        let slots = vec![slot(
            &mut slot_keys,
            StringTagCollection::from_strs(&["main"]),
            1,
            Some(1),
        )];

        let inner = MixAndMatchPromotion::new(
            key,
            slots,
            crate::promotions::mix_and_match::MixAndMatchDiscount::FixedTotal(Money::from_minor(
                100, GBP,
            )),
        );

        let promo = Promotion::MixAndMatch(inner);

        assert_eq!(promo.key(), key);
        assert_ne!(promo.key(), PromotionKey::default());
    }

    #[test]
    fn is_applicable_handles_mix_and_match() {
        let items: SmallVec<[Item<'_>; 10]> = SmallVec::from_vec(vec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["main"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(50, GBP),
                StringTagCollection::from_strs(&["drink"]),
            ),
        ]);

        let item_group: ItemGroup<'_> = ItemGroup::new(items, GBP);

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

        let inner = MixAndMatchPromotion::new(
            PromotionKey::default(),
            slots,
            crate::promotions::mix_and_match::MixAndMatchDiscount::PercentAllItems(
                decimal_percentage::Percentage::from(0.25),
            ),
        );

        let promo = Promotion::MixAndMatch(inner);

        assert!(promo.is_applicable(&item_group));
    }
}
