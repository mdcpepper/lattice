//! Positional Discount
//!
//! Promotions that apply discounts to specific positions when items are
//! ordered by price. This category encompasses BOGOF (2-for-1), BOGOHP
//! (second item half price), 3-for-2, 5-for-3, and similar X-for-Y offers.

use smallvec::SmallVec;

use crate::{
    discounts::SimpleDiscount,
    promotions::{PromotionKey, budget::PromotionBudget, qualification::Qualification},
    tags::{collection::TagCollection, string::StringTagCollection},
};

/// A Positional Discount Promotion
#[derive(Debug, Clone)]
pub struct PositionalDiscountPromotion<'a, T: TagCollection = StringTagCollection> {
    key: PromotionKey,
    qualification: Qualification<T>,
    size: u16,
    positions: SmallVec<[u16; 5]>,
    discount: SimpleDiscount<'a>,
    budget: PromotionBudget<'a>,
}

impl<'a, T: TagCollection> PositionalDiscountPromotion<'a, T> {
    /// Create a new positional discount promotion.
    pub fn new(
        key: PromotionKey,
        qualification: Qualification<T>,
        size: u16,
        positions: SmallVec<[u16; 5]>,
        discount: SimpleDiscount<'a>,
        budget: PromotionBudget<'a>,
    ) -> Self {
        Self {
            key,
            qualification,
            size,
            positions,
            discount,
            budget,
        }
    }

    /// Return the promotion key
    pub fn key(&self) -> PromotionKey {
        self.key
    }

    /// Return the item qualification expression.
    pub fn qualification(&self) -> &Qualification<T> {
        &self.qualification
    }

    /// Return the bundle size
    pub fn size(&self) -> u16 {
        self.size
    }

    /// Return the discount positions
    pub fn positions(&self) -> &[u16] {
        &self.positions
    }

    /// Return the discount
    pub fn discount(&self) -> &SimpleDiscount<'a> {
        &self.discount
    }

    /// Return the budget
    pub const fn budget(&self) -> &PromotionBudget<'a> {
        &self.budget
    }
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso::GBP};
    use smallvec::smallvec;

    use crate::{discounts::SimpleDiscount, tags::string::StringTagCollection};

    use super::*;

    #[test]
    fn accessors_return_constructor_values() {
        let key = PromotionKey::default();
        let qualification =
            Qualification::match_any(StringTagCollection::from_strs(&["sale", "vip"]));
        let positions = smallvec![0u16, 2u16];
        let discount = SimpleDiscount::AmountOff(Money::from_minor(50, GBP));

        let promo = PositionalDiscountPromotion::new(
            key,
            qualification.clone(),
            3,
            positions.clone(),
            discount,
            PromotionBudget::unlimited(),
        );

        assert_eq!(promo.key(), key);
        assert!(
            promo
                .qualification()
                .matches(&StringTagCollection::from_strs(&["vip"]))
        );
        assert_eq!(promo.qualification().rules.len(), qualification.rules.len());
        assert_eq!(promo.size(), 3);
        assert_eq!(promo.positions(), positions.as_slice());
        assert!(matches!(
            promo.discount(),
            SimpleDiscount::AmountOff(amount) if amount.to_minor_units() == 50
        ));
    }
}
