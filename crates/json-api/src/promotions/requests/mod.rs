//! Promotions Requests

use lattice_app::domain::promotions::{
    data::{NewPromotion, PromotionUpdate},
    records::PromotionUuid,
};
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

/// Update Promotion Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdatePromotionRequest {
    DirectDiscount {
        budgets: BudgetsRequest,
        discount: SimpleDiscountRequest,
        qualification: Option<CreateQualificationRequest>,
    },
}

impl From<UpdatePromotionRequest> for PromotionUpdate {
    fn from(request: UpdatePromotionRequest) -> Self {
        match request {
            UpdatePromotionRequest::DirectDiscount {
                budgets,
                discount,
                qualification,
            } => PromotionUpdate::DirectDiscount {
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

    use smallvec::smallvec;
    use testresult::TestResult;
    use uuid::Uuid;

    use crate::promotions::requests::qualification::{
        CreateQualificationRuleRequest, QualificationContextRequest, QualificationOpRequest,
    };

    use super::*;

    #[test]
    fn create_direct_discount_promotion_request_parse() -> TestResult {
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
                    "context": "primary",
                    "op": "and",
                    "rules": [
                        {
                            "type": "has_any",
                            "tags": ["include"]
                        },
                        {
                            "type": "has_none",
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
                    context: QualificationContextRequest::Primary,
                    op: QualificationOpRequest::And,
                    rules: vec![
                        CreateQualificationRuleRequest::HasAny {
                            tags: smallvec!["include".to_string()]
                        },
                        CreateQualificationRuleRequest::HasNone {
                            tags: smallvec!["except".to_string()]
                        }
                    ],
                })
            }
        );

        Ok(())
    }

    #[test]
    fn update_direct_discount_promotion_request_parse() -> TestResult {
        let json = r#"
            {
                "type": "direct_discount",
                "budgets": {
                    "redemptions": 500
                },
                "discount": {
                    "type": "percentage_off",
                    "percentage": 25
                },
                "qualification": {
                    "context": "primary",
                    "op": "and",
                    "rules": [
                        {
                            "type": "has_any",
                            "tags": ["include"]
                        },
                        {
                            "type": "has_none",
                            "tags": ["except"]
                        }
                    ]
                }
            }
        "#;

        let request: UpdatePromotionRequest = serde_json::from_str(json)?;

        assert_eq!(
            request,
            UpdatePromotionRequest::DirectDiscount {
                budgets: BudgetsRequest {
                    redemptions: Some(500),
                    monetary: None,
                },
                discount: SimpleDiscountRequest::PercentageOff { percentage: 25 },
                qualification: Some(CreateQualificationRequest {
                    context: QualificationContextRequest::Primary,
                    op: QualificationOpRequest::And,
                    rules: vec![
                        CreateQualificationRuleRequest::HasAny {
                            tags: smallvec!["include".to_string()]
                        },
                        CreateQualificationRuleRequest::HasNone {
                            tags: smallvec!["except".to_string()]
                        }
                    ],
                }),
            }
        );

        Ok(())
    }
}
