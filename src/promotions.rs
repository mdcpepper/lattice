//! Promotions

use crate::{
    basket::Basket, promotions::simple_discount::SimpleDisount,
    solvers::ilp::promotions::ILPPromotion,
};

pub mod simple_discount;

/// Promotion enum
#[derive(Debug, Clone)]
pub enum Promotion<'a> {
    /// Simple discount promotion
    SimpleDiscount(SimpleDisount<'a>),
}

impl<'a> Promotion<'a> {
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
    use testresult::TestResult;

    use crate::{
        basket::Basket,
        discounts::Discount,
        tags::{collection::TagCollection, string::StringTagCollection},
    };

    use super::{Promotion, simple_discount::SimpleDisount};

    #[test]
    fn is_applicable_delegates_to_inner_promotion() -> TestResult {
        // An empty item set should not be considered applicable; this ensures
        // `Promotion::is_applicable` doesn't accidentally short-circuit to `true`.
        let basket = Basket::with_items([], iso::GBP)?;

        let inner = SimpleDisount::new(
            StringTagCollection::empty(),
            Discount::SetBundleTotalPrice(Money::from_minor(50, iso::GBP)),
        );

        let promo = Promotion::SimpleDiscount(inner);

        assert!(!promo.is_applicable(&basket, &[]));

        Ok(())
    }
}
