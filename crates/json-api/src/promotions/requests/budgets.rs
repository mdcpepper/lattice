//! Promotion Budgets Request

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetsRequest {
    pub application_budget: Option<i64>,
    pub monetary_budget: Option<i64>,
}
