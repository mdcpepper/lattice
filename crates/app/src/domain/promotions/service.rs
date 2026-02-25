//! Promotions Service

use crate::{
    database::Db,
    domain::{
        promotions::{
            PromotionsServiceError,
            data::Promotion,
            records::PromotionRecord,
            repositories::{
                promotions::PgPromotionsRepository, qualifications::PgQualificationsRepository,
            },
        },
        tenants::records::TenantUuid,
    },
};
use async_trait::async_trait;
use mockall::automock;

#[derive(Debug, Clone)]
pub struct PgPromotionsService {
    db: Db,
    promotions_repository: PgPromotionsRepository,
    qualifications_repository: PgQualificationsRepository,
}

impl PgPromotionsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            promotions_repository: PgPromotionsRepository::new(),
            qualifications_repository: PgQualificationsRepository::new(),
        }
    }
}

#[async_trait]
impl PromotionsService for PgPromotionsService {
    async fn create_promotion(
        &self,
        tenant: TenantUuid,
        promotion: Promotion,
    ) -> Result<PromotionRecord, PromotionsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;
        let mut promotion = promotion;

        let (promotion_uuid, qualification) = match &mut promotion {
            Promotion::DirectDiscount {
                uuid,
                qualification,
                ..
            } => (*uuid, qualification.take()),
        };

        let record = self
            .promotions_repository
            .create_promotion(&mut tx, promotion)
            .await?;

        if let Some(qual) = qualification {
            self.qualifications_repository
                .create_qualifications(&mut tx, promotion_uuid, &qual)
                .await?;
        }

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
        promotion: Promotion,
    ) -> Result<PromotionRecord, PromotionsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use smallvec::smallvec;
    use testresult::TestResult;

    use crate::{
        domain::promotions::{
            data::{
                Promotion,
                budgets::Budgets,
                discounts::SimpleDiscount,
                qualification::{
                    Qualification, QualificationContext, QualificationOp, QualificationRule,
                },
            },
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
                Promotion::DirectDiscount {
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
                Promotion::DirectDiscount {
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
                Promotion::DirectDiscount {
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
                Promotion::DirectDiscount {
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
                Promotion::DirectDiscount {
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
                Promotion::DirectDiscount {
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

    #[tokio::test]
    async fn create_promotion_with_qualification_succeeds() -> TestResult {
        let ctx = TestContext::new().await;

        let result = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                Promotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 10 },
                    qualification: Some(Qualification {
                        context: QualificationContext::Primary,
                        op: QualificationOp::And,
                        rules: vec![QualificationRule::HasAny {
                            tags: smallvec!["sale".to_string()],
                        }],
                    }),
                },
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn create_promotion_with_nested_qualification_succeeds() -> TestResult {
        let ctx = TestContext::new().await;

        let result = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                Promotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 10 },
                    qualification: Some(Qualification {
                        context: QualificationContext::Primary,
                        op: QualificationOp::Or,
                        rules: vec![
                            QualificationRule::HasAll {
                                tags: smallvec!["clothing".to_string(), "sale".to_string()],
                            },
                            QualificationRule::Group {
                                qualification: Qualification {
                                    context: QualificationContext::Group,
                                    op: QualificationOp::And,
                                    rules: vec![
                                        QualificationRule::HasAny {
                                            tags: smallvec!["footwear".to_string()],
                                        },
                                        QualificationRule::HasNone {
                                            tags: smallvec!["excluded".to_string()],
                                        },
                                    ],
                                },
                            },
                        ],
                    }),
                },
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }
}
