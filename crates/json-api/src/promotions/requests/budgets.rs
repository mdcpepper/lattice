//! Promotion Budgets Request

use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

use lattice_app::domain::promotions::data::budgets::Budgets;

/// Budgets Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct BudgetsRequest {
    pub redemptions: Option<u64>,
    pub monetary: Option<u64>,
}

impl From<BudgetsRequest> for Budgets {
    fn from(request: BudgetsRequest) -> Self {
        Budgets {
            redemptions: request.redemptions,
            monetary: request.monetary,
        }
    }
}
