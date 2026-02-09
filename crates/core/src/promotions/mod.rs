//! Promotions

use std::sync::Arc;

use slotmap::{SecondaryMap, new_key_type};

use crate::{graph::PromotionLayerKey, solvers::ilp::ILPPromotion};

pub mod applications;
pub mod budget;
pub mod prelude;
pub mod types;

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

    /// Layer names
    pub layer_names: SecondaryMap<PromotionLayerKey, String>,
}

/// Promotion object used by solvers and graph layers.
pub type Promotion<'a> = Arc<dyn ILPPromotion + 'a>;

/// Convert any ILP-capable promotion implementation into a shared promotion object.
pub fn promotion<'a, P>(promotion: P) -> Promotion<'a>
where
    P: ILPPromotion + 'a,
{
    Arc::new(promotion)
}

#[cfg(test)]
mod tests {
    use rusty_money::{Money, iso::GBP};

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::ProductKey,
        promotions::{PromotionKey, budget::PromotionBudget, types::DirectDiscountPromotion},
        tags::string::StringTagCollection,
    };

    use super::*;

    #[test]
    fn promotion_helper_wraps_trait_implementation() {
        let key = PromotionKey::default();
        let wrapped = promotion(DirectDiscountPromotion::new(
            key,
            StringTagCollection::from_strs(&["sale"]),
            SimpleDiscount::AmountOverride(Money::from_minor(50, GBP)),
            PromotionBudget::unlimited(),
        ));

        assert_eq!(wrapped.key(), key);

        let items = [Item::with_tags(
            ProductKey::default(),
            Money::from_minor(100, GBP),
            StringTagCollection::from_strs(&["sale"]),
        )];
        let item_group = ItemGroup::new(items.into_iter().collect(), GBP);

        assert!(wrapped.is_applicable(&item_group));
    }
}
