//! Promotion Discount Requests

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SimpleDiscountRequest {
    PercentageOff { percentage: i32 },
    FixedAmountOff { amount: i64 },
}
