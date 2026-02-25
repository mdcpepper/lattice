//! Promotion Budgets Data

/// Promotion Budgets Data
#[derive(Debug, Clone, PartialEq)]
pub struct Budgets {
    pub redemptions: Option<u64>,
    pub monetary: Option<u64>,
}
