//! Promotions Service

use async_trait::async_trait;
use mockall::automock;

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
        tags::PgTagsRepository,
        tenants::records::TenantUuid,
    },
};

#[derive(Debug, Clone)]
pub struct PgPromotionsService {
    db: Db,
    promotions: PgPromotionsRepository,
    qualifications: PgQualificationsRepository,
    tags: PgTagsRepository,
}

impl PgPromotionsService {
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self {
            db,
            promotions: PgPromotionsRepository::new(),
            qualifications: PgQualificationsRepository::new(),
            tags: PgTagsRepository::new(),
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

        let record = self.promotions.create_promotion(&mut tx, promotion).await?;

        if let Some(qual) = qualification {
            let rule_tags = self
                .qualifications
                .create_qualifications(&mut tx, promotion_uuid, &qual)
                .await?;

            let taggables = self.tags.resolve_taggable_tags(&mut tx, &rule_tags).await?;

            self.tags.create_taggables(&mut tx, &taggables).await?;
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
    use sqlx::{Row, postgres::PgRow};
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ExpectedDetail {
        redemption_budget: Option<i64>,
        monetary_budget: Option<i64>,
        kind: String,
        discount_percentage: Option<i64>,
        discount_amount: Option<i64>,
    }

    impl<'r> sqlx::FromRow<'r, PgRow> for ExpectedDetail {
        fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
            Ok(Self {
                redemption_budget: row.try_get("redemption_budget")?,
                monetary_budget: row.try_get("monetary_budget")?,
                kind: row.try_get("kind")?,
                discount_percentage: row.try_get("discount_percentage")?,
                discount_amount: row.try_get("discount_amount")?,
            })
        }
    }

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
    async fn create_promotion_inserts_rows_as_expected() -> TestResult {
        let ctx = TestContext::new().await;

        let promotion = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                Promotion::DirectDiscount {
                    uuid: PromotionUuid::new(),
                    budgets: Budgets {
                        redemptions: Some(7),
                        monetary: Some(9_000),
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 350 },
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
            .await?;

        let detail: ExpectedDetail = sqlx::query_as(
            "SELECT
               redemption_budget,
               monetary_budget,
               discount_kind::text AS kind,
               discount_percentage,
               discount_amount
             FROM direct_discount_promotion_details
             WHERE promotion_uuid = $1
               AND upper_inf(valid_period)",
        )
        .bind(promotion.uuid.into_uuid())
        .fetch_one(ctx.db.pool())
        .await?;

        assert_eq!(
            detail,
            ExpectedDetail {
                redemption_budget: Some(7),
                monetary_budget: Some(9_000),
                kind: "amount_off".to_string(),
                discount_percentage: None,
                discount_amount: Some(350),
            }
        );

        let qualifications: Vec<(String, String, bool)> = sqlx::query_as(
            "SELECT
               context::text,
               op::text,
               parent_qualification_uuid IS NOT NULL AS has_parent
             FROM qualifications
             WHERE promotion_uuid = $1
             ORDER BY parent_qualification_uuid IS NOT NULL, context::text",
        )
        .bind(promotion.uuid.into_uuid())
        .fetch_all(ctx.db.pool())
        .await?;

        assert_eq!(
            qualifications,
            vec![
                ("primary".to_string(), "or".to_string(), false),
                ("group".to_string(), "and".to_string(), true),
            ]
        );

        let rule_tags: Vec<(String, String)> = sqlx::query_as(
            "SELECT qr.kind::text, t.name
             FROM qualification_rules qr
             JOIN qualifications q ON q.uuid = qr.qualification_uuid
             JOIN taggables tg
               ON tg.taggable_uuid = qr.uuid
              AND tg.taggable_type = 'qualification_rule'
             JOIN tags t ON t.uuid = tg.tag_uuid
             WHERE q.promotion_uuid = $1
             ORDER BY qr.kind::text, t.name",
        )
        .bind(promotion.uuid.into_uuid())
        .fetch_all(ctx.db.pool())
        .await?;

        assert_eq!(
            rule_tags,
            vec![
                ("has_all".to_string(), "clothing".to_string()),
                ("has_all".to_string(), "sale".to_string()),
                ("has_any".to_string(), "footwear".to_string()),
                ("has_none".to_string(), "excluded".to_string()),
            ]
        );

        Ok(())
    }
}
