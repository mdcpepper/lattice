//! Promotions Requests

use serde::{Deserialize, Serialize};

use crate::promotions::requests::{
    budgets::BudgetsRequest, discounts::SimpleDiscountRequest, qualification::QualificationRequest,
};

pub(crate) mod budgets;
pub(crate) mod discounts;
pub(crate) mod qualification;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromotionRequest {
    DirectDiscount {
        budgets: BudgetsRequest,
        discount: SimpleDiscountRequest,
        qualification: Option<QualificationRequest>,
    },
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use testresult::TestResult;
    use uuid::Uuid;

    use crate::promotions::requests::qualification::{
        QualificationContextRequest, QualificationOpRequest, QualificationRuleRequest,
    };

    use super::*;

    #[test]
    fn direct_discount_promotion_request_parse() -> TestResult {
        let json = r#"
            {
                "type": "direct_discount",
                "budgets": {
                    "application_budget": 1000,
                    "monetary_budget": 5000
                },
                "discount": {
                    "type": "percentage_off",
                    "percentage": 50
                },
                "qualification": {
                    "uuid": "019c8e09-321a-7b0e-8ad1-27a98a4e4dc5",
                    "context": "primary",
                    "op": "and",
                    "rules": [
                        {
                            "kind": "has_any",
                            "uuid": "019c8e0b-485e-7a3e-80dc-a500783fc2e1",
                            "tags": ["include"]
                        },
                        {
                            "kind": "has_none",
                            "uuid": "019c8e0b-5916-71bc-997b-3bd2e613af45",
                            "tags": ["except"]
                        }
                    ]
                }
            }
        "#;

        let promotion_request: PromotionRequest = serde_json::from_str(json)?;

        assert_eq!(
            promotion_request,
            PromotionRequest::DirectDiscount {
                budgets: BudgetsRequest {
                    application_budget: Some(1000),
                    monetary_budget: Some(5000),
                },
                discount: SimpleDiscountRequest::PercentageOff { percentage: 50 },
                qualification: Some(QualificationRequest {
                    uuid: Uuid::from_str("019c8e09-321a-7b0e-8ad1-27a98a4e4dc5")?,
                    context: QualificationContextRequest::Primary,
                    op: QualificationOpRequest::And,
                    rules: vec![
                        QualificationRuleRequest::HasAny {
                            uuid: Uuid::from_str("019c8e0b-485e-7a3e-80dc-a500783fc2e1")?,
                            tags: vec!["include".to_string()]
                        },
                        QualificationRuleRequest::HasNone {
                            uuid: Uuid::from_str("019c8e0b-5916-71bc-997b-3bd2e613af45")?,
                            tags: vec!["except".to_string()]
                        }
                    ],
                })
            }
        );

        Ok(())
    }
}
