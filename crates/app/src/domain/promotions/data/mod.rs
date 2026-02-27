//! Promotions Data

use crate::domain::promotions::{
    data::{budgets::Budgets, discounts::SimpleDiscount, qualification::Qualification},
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
        qualification: Option<Qualification>,
    },
}

impl NewPromotion {
    #[must_use]
    pub const fn type_as_str(&self) -> &'static str {
        match self {
            Self::DirectDiscount { .. } => "direct",
        }
    }

    #[must_use]
    pub fn uuid(&self) -> PromotionUuid {
        match self {
            Self::DirectDiscount { uuid, .. } => *uuid,
        }
    }

    pub fn take_qualification(&mut self) -> Option<Qualification> {
        match self {
            Self::DirectDiscount { qualification, .. } => qualification.take(),
        }
    }
}

/// Promotion Update Data
#[derive(Debug, Clone, PartialEq)]
pub enum PromotionUpdate {
    DirectDiscount {
        budgets: Budgets,
        discount: SimpleDiscount,
        qualification: Option<Qualification>,
    },
}

impl PromotionUpdate {
    #[must_use]
    pub const fn type_as_str(&self) -> &'static str {
        match self {
            Self::DirectDiscount { .. } => "direct",
        }
    }

    pub fn take_qualification(&mut self) -> Option<Qualification> {
        match self {
            Self::DirectDiscount { qualification, .. } => qualification.take(),
        }
    }
}
