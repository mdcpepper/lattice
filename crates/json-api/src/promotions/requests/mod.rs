//! Promotions Requests

use lattice_app::domain::promotions::{data::NewPromotion, records::PromotionUuid};
use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::promotions::requests::{
    budgets::BudgetsRequest, discounts::SimpleDiscountRequest,
    qualification::CreateQualificationRequest,
};

pub(crate) mod budgets;
pub(crate) mod discounts;
pub(crate) mod qualification;

/// Create Promotion Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CreatePromotionRequest {
    DirectDiscount {
        uuid: Uuid,
        budgets: BudgetsRequest,
        discount: SimpleDiscountRequest,
        qualification: Option<CreateQualificationRequest>,
    },
}

impl From<CreatePromotionRequest> for NewPromotion {
    fn from(request: CreatePromotionRequest) -> Self {
        match request {
            CreatePromotionRequest::DirectDiscount {
                uuid,
                budgets,
                discount,
                qualification,
            } => NewPromotion::DirectDiscount {
                uuid: PromotionUuid::from_uuid(uuid),
                budgets: budgets.into(),
                discount: discount.into(),
                qualification: qualification.map(Into::into),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use testresult::TestResult;
    use uuid::Uuid;

    use crate::promotions::requests::qualification::{
        CreateQualificationRuleRequest, QualificationContextRequest, QualificationOpRequest,
    };

    use super::*;

    #[test]
    fn direct_discount_promotion_request_parse() -> TestResult {
        let json = r#"
            {
                "type": "direct_discount",
                "uuid": "019c8e08-0000-7000-8000-000000000001",
                "budgets": {
                    "redemptions": 1000,
                    "monetary": 5000
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

        let promotion_request: CreatePromotionRequest = serde_json::from_str(json)?;

        assert_eq!(
            promotion_request,
            CreatePromotionRequest::DirectDiscount {
                uuid: Uuid::from_str("019c8e08-0000-7000-8000-000000000001")?,
                budgets: BudgetsRequest {
                    redemptions: Some(1000),
                    monetary: Some(5000),
                },
                discount: SimpleDiscountRequest::PercentageOff { percentage: 50 },
                qualification: Some(CreateQualificationRequest {
                    uuid: Uuid::from_str("019c8e09-321a-7b0e-8ad1-27a98a4e4dc5")?,
                    context: QualificationContextRequest::Primary,
                    op: QualificationOpRequest::And,
                    rules: vec![
                        CreateQualificationRuleRequest::HasAny {
                            uuid: Uuid::from_str("019c8e0b-485e-7a3e-80dc-a500783fc2e1")?,
                            tags: vec!["include".to_string()]
                        },
                        CreateQualificationRuleRequest::HasNone {
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
