//! Promotions

use slotmap::new_key_type;

use crate::{
    basket::Basket, promotions::simple_discount::SimpleDiscount,
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

impl<'a> Promotion<'a> {
    /// Return the promotion key.
    pub fn key(&self) -> PromotionKey {
        match self {
            Promotion::SimpleDiscount(simple_discount) => simple_discount.key(),
        }
    }

    /// Return whether this promotion *might* apply to the given basket and candidate items.
    pub fn is_applicable(&self, basket: &'a Basket<'a>, items: &[usize]) -> bool {
        match self {
            Promotion::SimpleDiscount(simple_disount) => {
                simple_disount.is_applicable(basket, items)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso};
    use slotmap::SlotMap;
    use testresult::TestResult;

    use crate::{
        basket::Basket,
        discounts::Discount,
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::{Promotion, PromotionKey, simple_discount::SimpleDiscount};

    #[test]
    fn key_delegates_to_inner_promotion_key() -> TestResult {
        let basket = Basket::with_items([], iso::GBP)?;

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
        let _ = promo.is_applicable(&basket, &[]);

        Ok(())
    }

    #[test]
    fn is_applicable_delegates_to_inner_promotion() -> TestResult {
        // An empty item set should not be considered applicable; this ensures
        // `Promotion::is_applicable` doesn't accidentally short-circuit to `true`.
        let basket = Basket::with_items([], iso::GBP)?;

        let inner = SimpleDiscount::new(
            PromotionKey::default(),
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::SimpleDiscount(inner);

        assert!(!promo.is_applicable(&basket, &[]));

        Ok(())
    }
}
