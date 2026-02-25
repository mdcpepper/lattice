//! Promotions Data

use crate::domain::promotions::{
    data::{budgets::Budgets, discounts::SimpleDiscount, qualification::Qualification},
    records::PromotionUuid,
};

pub mod budgets;
pub mod discounts;
pub mod qualification;

/// Promotion Data
#[derive(Debug, Clone, PartialEq)]
pub enum Promotion {
    DirectDiscount {
        uuid: PromotionUuid,
        budgets: Budgets,
        discount: SimpleDiscount,
        qualification: Option<Qualification>,
    },
}

impl Promotion {
    #[must_use]
    pub const fn type_as_str(&self) -> &'static str {
        match self {
            Self::DirectDiscount { .. } => "direct",
        }
    }
}
