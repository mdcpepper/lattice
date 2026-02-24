//! Promotions Data

use crate::domain::promotions::{
    data::{budgets::Budgets, discounts::SimpleDiscount, qualification::NewQualification},
    records::PromotionUuid,
};

pub mod budgets;
pub mod discounts;
pub mod qualification;

/// New Promotion Data
#[derive(Debug, Clone, PartialEq)]
pub enum NewPromotion {
    DirectDiscount {
        uuid: PromotionUuid,
        budgets: Budgets,
        discount: SimpleDiscount,
        qualification: Option<NewQualification>,
    },
}

impl NewPromotion {
    #[must_use]
    pub const fn kind_to_str(&self) -> &'static str {
        match self {
            Self::DirectDiscount { .. } => "direct",
        }
    }
}
