//! Create Promotion Handler

use std::sync::Arc;

use salvo::{Depot, http::header::LOCATION, oapi::extract::JsonBody, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    extensions::*,
    promotions::{errors::into_status_error, requests::CreatePromotionRequest},
    state::State,
};

/// Promotion Created Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PromotionCreatedResponse {
    /// Created promotion UUID
    pub uuid: Uuid,
}

/// Create Promotion Handler
#[endpoint(
    tags("promotions"),
    summary = "Create Promotion",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::CREATED, description = "Promotion created"),
        (status_code = StatusCode::CONFLICT, description = "Promotion already exists"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    json: JsonBody<CreatePromotionRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<PromotionCreatedResponse>, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;

    let uuid = state
        .app
        .promotions
        .create_promotion(tenant, json.into_inner().into())
        .await
        .map_err(into_status_error)?
        .uuid;

    res.add_header(LOCATION, format!("/promotions/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::CREATED);

    Ok(Json(PromotionCreatedResponse { uuid: uuid.into() }))
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use salvo::test::{ResponseExt, TestClient};
    use serde_json::json;
    use smallvec::smallvec;
    use testresult::TestResult;

    use lattice_app::domain::promotions::{
        PromotionsServiceError,
        data::{
            Promotion,
            budgets::Budgets,
            discounts::SimpleDiscount,
            qualification::{
                Qualification, QualificationContext, QualificationOp, QualificationRule,
            },
        },
        records::{PromotionRecord, PromotionUuid},
        service::MockPromotionsService,
    };

    use crate::test_helpers::{TEST_TENANT_UUID, promotions_service};

    use super::*;

    fn make_service(promotions: MockPromotionsService) -> Service {
        promotions_service(promotions, Router::with_path("promotions").post(handler))
    }

    fn make_promotion(uuid: PromotionUuid) -> PromotionRecord {
        PromotionRecord {
            uuid,
            created_at: Timestamp::UNIX_EPOCH,
            updated_at: Timestamp::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn test_create_promotion_success() -> TestResult {
        let promotion_uuid = PromotionUuid::new();
        let promotion = make_promotion(promotion_uuid);

        let mut mock = MockPromotionsService::new();

        mock.expect_create_promotion()
            .once()
            .withf(move |tenant, new| {
                *tenant == TEST_TENANT_UUID
                    && *new
                        == Promotion::DirectDiscount {
                            uuid: promotion_uuid,
                            budgets: Budgets {
                                redemptions: Some(100),
                                monetary: None,
                            },
                            discount: SimpleDiscount::PercentageOff { percentage: 10 },
                            qualification: Some(Qualification {
                                context: QualificationContext::Primary,
                                op: QualificationOp::And,
                                rules: vec![
                                    QualificationRule::HasAny {
                                        tags: smallvec!["included".to_string()],
                                    },
                                    QualificationRule::HasNone {
                                        tags: smallvec!["excluded".to_string()],
                                    },
                                ],
                            }),
                        }
            })
            .return_once(move |_, _| Ok(promotion));

        let mut res = TestClient::post("http://example.com/promotions")
            .json(&json!({
                "type": "direct_discount",
                "uuid": promotion_uuid.into_uuid(),
                "budgets": { "redemptions": 100 },
                "discount": { "type": "percentage_off", "percentage": 10 },
                "qualification": {
                    "uuid": promotion_uuid.into_uuid(),
                    "op": "and",
                    "rules": [
                        {
                            "type": "has_any",
                            "tags": ["included"]
                        },
                        {
                            "type": "has_none",
                            "tags": ["excluded"]
                        }
                    ]
                }
            }))
            .send(&make_service(mock))
            .await;

        let body: PromotionCreatedResponse = res.take_json().await?;
        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::CREATED));
        assert_eq!(
            location,
            Some(format!("/promotions/{promotion_uuid}").as_str())
        );
        assert_eq!(body.uuid, promotion_uuid.into_uuid());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_promotion_conflict_returns_409() -> TestResult {
        let uuid = PromotionUuid::new();

        let mut mock = MockPromotionsService::new();

        mock.expect_create_promotion()
            .once()
            .return_once(|_, _| Err(PromotionsServiceError::AlreadyExists));

        let res = TestClient::post("http://example.com/promotions")
            .json(&json!({
                "type": "direct_discount",
                "uuid": uuid.into_uuid(),
                "budgets": {},
                "discount": { "type": "percentage_off", "percentage": 10 }
            }))
            .send(&make_service(mock))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::CONFLICT));

        Ok(())
    }
}
