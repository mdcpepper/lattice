//! Promotions Service

use crate::{
    database::Db,
    domain::{
        promotions::{
            PromotionsServiceError, data::NewPromotion, records::PromotionRecord,
            repositories::promotions::PgPromotionsRepository,
        },
        tenants::records::TenantUuid,
    },
};
use async_trait::async_trait;
use mockall::automock;

#[derive(Debug, Clone)]
pub struct PgPromotionsService {
    db: Db,
    repository: PgPromotionsRepository,
}

impl PgPromotionsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            repository: PgPromotionsRepository::new(),
        }
    }
}

#[async_trait]
impl PromotionsService for PgPromotionsService {
    async fn create_promotion(
        &self,
        tenant: TenantUuid,
        promotion: NewPromotion,
    ) -> Result<PromotionRecord, PromotionsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let record = self.repository.create_promotion(&mut tx, promotion).await?;

        tx.commit().await?;

        Ok(record)
    }
}

#[automock]
#[async_trait]
pub trait PromotionsService: Send + Sync {
    async fn create_promotion(
        &self,
        tenant: TenantUuid,
        promotion: NewPromotion,
    ) -> Result<PromotionRecord, PromotionsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use testresult::TestResult;

    use crate::{
        domain::promotions::{
            data::{NewPromotion, budgets::Budgets, discounts::SimpleDiscount},
            records::PromotionUuid,
        },
        test::TestContext,
    };

    use super::*;

    #[tokio::test]
    async fn create_promotion_returns_correct_uuid() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = PromotionUuid::new();

        let promotion = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid,
                    budgets: Budgets {
                        redemptions: Some(100),
                        monetary: Some(10_000),
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 20 },
                    qualification: None,
                },
            )
            .await?;

        assert_eq!(promotion.uuid, uuid);
        assert!(promotion.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn create_promotion_timestamps_are_set() -> TestResult {
        let ctx = TestContext::new().await;

        let before = Timestamp::now();

        let promotion = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 500 },
                    qualification: None,
                },
            )
            .await?;

        let after = Timestamp::now();

        assert!(promotion.created_at >= before);
        assert!(promotion.created_at <= after);

        Ok(())
    }

    #[tokio::test]
    async fn create_promotion_duplicate_uuid_returns_already_exists() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = PromotionUuid::new();

        ctx.promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid,
                    budgets: Budgets {
                        redemptions: Some(10),
                        monetary: Some(10_000),
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 10 },
                    qualification: None,
                },
            )
            .await?;

        let result = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid,
                    budgets: Budgets {
                        redemptions: Some(20),
                        monetary: Some(20_000),
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 200 },
                    qualification: None,
                },
            )
            .await;

        assert!(
            matches!(result, Err(PromotionsServiceError::AlreadyExists)),
            "expected AlreadyExists, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn create_promotion_zero_percentage_returns_invalid_data() {
        let ctx = TestContext::new().await;

        let result = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 0 },
                    qualification: None,
                },
            )
            .await;

        assert!(
            matches!(result, Err(PromotionsServiceError::InvalidData)),
            "expected InvalidData, got {result:?}"
        );
    }

    #[tokio::test]
    async fn create_promotion_zero_amount_returns_invalid_data() {
        let ctx = TestContext::new().await;

        let result = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 0 },
                    qualification: None,
                },
            )
            .await;

        assert!(
            matches!(result, Err(PromotionsServiceError::InvalidData)),
            "expected InvalidData, got {result:?}"
        );
    }
}
