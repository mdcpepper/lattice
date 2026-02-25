//! Promotion Discounts

/// Simple Discount Data
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleDiscount {
    PercentageOff { percentage: u16 },
    FixedAmountOff { amount: u64 },
}

impl SimpleDiscount {
    #[must_use]
    pub const fn to_str(&self) -> &'static str {
        match self {
            Self::PercentageOff { .. } => "percentage_off",
            Self::FixedAmountOff { .. } => "amount_off",
        }
    }
}
