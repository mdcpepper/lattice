//! Promotion Discount Requests

use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

use lattice_app::domain::promotions::data::discounts::SimpleDiscount;

/// Simple Discount Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimpleDiscountRequest {
    PercentageOff { percentage: u16 },
    FixedAmountOff { amount: u64 },
}

impl From<SimpleDiscountRequest> for SimpleDiscount {
    fn from(request: SimpleDiscountRequest) -> Self {
        match request {
            SimpleDiscountRequest::PercentageOff { percentage } => {
                SimpleDiscount::PercentageOff { percentage }
            }
            SimpleDiscountRequest::FixedAmountOff { amount } => {
                SimpleDiscount::FixedAmountOff { amount }
            }
        }
    }
}
