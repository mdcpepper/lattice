//! Update Promotion Handler

use std::sync::Arc;

use salvo::{
    Depot,
    http::header::LOCATION,
    oapi::extract::{JsonBody, PathParam},
    prelude::*,
};
use uuid::Uuid;

use lattice_app::domain::promotions::records::PromotionUuid;

use crate::{
    extensions::*,
    promotions::{errors::into_status_error, requests::UpdatePromotionRequest},
    state::State,
};

/// Update Promotion Handler
#[endpoint(
    tags("promotions"),
    summary = "Update Promotion",
    security(("bearer_auth" = [])),
    responses(
        (status_code = StatusCode::OK, description = "Promotion updated"),
        (status_code = StatusCode::NOT_FOUND, description = "Promotion not found"),
        (status_code = StatusCode::BAD_REQUEST, description = "Bad Request"),
        (status_code = StatusCode::INTERNAL_SERVER_ERROR, description = "Internal Server Error"),
    ),
)]
pub(crate) async fn handler(
    uuid: PathParam<Uuid>,
    json: JsonBody<UpdatePromotionRequest>,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<StatusCode, StatusError> {
    let state = depot.obtain_or_500::<Arc<State>>()?;
    let tenant = depot.tenant_uuid_or_401()?;
    let uuid = uuid.into_inner();

    state
        .app
        .promotions
        .update_promotion(
            tenant,
            PromotionUuid::from_uuid(uuid),
            json.into_inner().into(),
        )
        .await
        .map_err(into_status_error)?;

    res.add_header(LOCATION, format!("/promotions/{uuid}"), true)
        .or_500("failed to set location header")?
        .status_code(StatusCode::OK);

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use salvo::test::TestClient;
    use serde_json::json;
    use testresult::TestResult;

    use lattice_app::domain::promotions::{
        PromotionsServiceError,
        data::{PromotionUpdate, budgets::Budgets, discounts::SimpleDiscount},
        records::PromotionUuid,
        service::MockPromotionsService,
    };

    use crate::test_helpers::{TEST_TENANT_UUID, promotions_service};

    use super::*;

    fn make_service(promotions: MockPromotionsService) -> Service {
        promotions_service(
            promotions,
            Router::with_path("promotions/{uuid}").put(handler),
        )
    }

    #[tokio::test]
    async fn test_update_promotion_success() -> TestResult {
        let promotion_uuid = PromotionUuid::new();

        let mut mock = MockPromotionsService::new();

        mock.expect_update_promotion()
            .once()
            .withf(move |tenant, uuid, update| {
                *tenant == TEST_TENANT_UUID
                    && *uuid == promotion_uuid
                    && *update
                        == PromotionUpdate::DirectDiscount {
                            budgets: Budgets {
                                redemptions: Some(50),
                                monetary: None,
                            },
                            discount: SimpleDiscount::PercentageOff { percentage: 10 },
                            qualification: None,
                        }
            })
            .return_once(move |_, _, _| Ok(()));

        let res = TestClient::put(format!(
            "http://example.com/promotions/{}",
            promotion_uuid.into_uuid()
        ))
        .json(&json!({
            "type": "direct_discount",
            "budgets": { "redemptions": 50 },
            "discount": { "type": "percentage_off", "percentage": 10 }
        }))
        .send(&make_service(mock))
        .await;

        let location = res.headers().get("location").and_then(|v| v.to_str().ok());

        assert_eq!(res.status_code, Some(StatusCode::OK));
        assert_eq!(
            location,
            Some(format!("/promotions/{promotion_uuid}").as_str())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_update_promotion_not_found_returns_404() -> TestResult {
        let promotion_uuid = PromotionUuid::new();

        let mut mock = MockPromotionsService::new();

        mock.expect_update_promotion()
            .once()
            .return_once(|_, _, _| Err(PromotionsServiceError::NotFound));

        let res = TestClient::put(format!(
            "http://example.com/promotions/{}",
            promotion_uuid.into_uuid()
        ))
        .json(&json!({
            "type": "direct_discount",
            "budgets": {},
            "discount": { "type": "percentage_off", "percentage": 10 }
        }))
        .send(&make_service(mock))
        .await;

        assert_eq!(res.status_code, Some(StatusCode::NOT_FOUND));

        Ok(())
    }
}
