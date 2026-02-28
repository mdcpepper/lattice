//! Promotions Service

use async_trait::async_trait;
use mockall::automock;
use tracing::{Span, info};

use crate::{
    database::Db,
    domain::{
        promotions::{
            PromotionsServiceError,
            data::{NewPromotion, PromotionUpdate},
            records::{DirectDiscountDetailUuid, PromotionRecord, PromotionUuid},
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
    #[tracing::instrument(
        name = "promotions.service.create_promotion",
        skip(self, promotion),
        fields(
            tenant_uuid = %tenant,
            promotion_uuid = tracing::field::Empty,
            promotion_type = tracing::field::Empty,
            has_qualification = tracing::field::Empty,
            rule_tag_count = tracing::field::Empty
        ),
        err
    )]
    async fn create_promotion(
        &self,
        tenant: TenantUuid,
        promotion: NewPromotion,
    ) -> Result<PromotionRecord, PromotionsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let mut promotion = promotion;

        let promotion_uuid = promotion.uuid();
        let promotionable_type = promotion.type_as_str();
        let qualification = promotion.take_qualification();

        let span = Span::current();

        span.record("promotion_uuid", tracing::field::display(promotion_uuid));

        span.record(
            "promotion_type",
            tracing::field::display(promotionable_type),
        );

        span.record(
            "has_qualification",
            tracing::field::display(qualification.is_some()),
        );

        let detail_uuid = DirectDiscountDetailUuid::from_uuid(promotion_uuid.into_uuid());

        let record = self.promotions.create_promotion(&mut tx, promotion).await?;

        if let Some(qual) = qualification {
            let rule_tags = self
                .qualifications
                .create_qualifications(
                    &mut tx,
                    promotion_uuid,
                    detail_uuid,
                    promotionable_type,
                    &qual,
                )
                .await?;

            let rule_tag_count = rule_tags.len();

            span.record("rule_tag_count", tracing::field::display(rule_tag_count));

            let taggables = self.tags.resolve_taggable_tags(&mut tx, &rule_tags).await?;

            self.tags.create_taggables(&mut tx, &taggables).await?;
        }

        tx.commit().await?;

        info!(promotion_uuid = %record.uuid, "created promotion");

        Ok(record)
    }

    #[tracing::instrument(
        name = "promotions.service.update_promotion",
        skip(self, update),
        fields(
            tenant_uuid = %tenant,
            promotion_uuid = %uuid,
            promotion_type = tracing::field::Empty,
            has_qualification = tracing::field::Empty,
            detail_uuid = tracing::field::Empty,
            rule_tag_count = tracing::field::Empty
        ),
        err
    )]
    async fn update_promotion(
        &self,
        tenant: TenantUuid,
        uuid: PromotionUuid,
        update: PromotionUpdate,
    ) -> Result<(), PromotionsServiceError> {
        let mut tx = self.db.begin_tenant_transaction(tenant).await?;

        let mut update = update;

        let promotionable_type = update.type_as_str();
        let qualification = update.take_qualification();

        let span = Span::current();

        span.record(
            "promotion_type",
            tracing::field::display(promotionable_type),
        );

        span.record(
            "has_qualification",
            tracing::field::display(qualification.is_some()),
        );

        let detail_uuid = self
            .promotions
            .update_promotion(&mut tx, uuid, update)
            .await?;

        span.record("detail_uuid", tracing::field::display(detail_uuid));

        if let Some(qual) = qualification {
            let rule_tags = self
                .qualifications
                .create_qualifications(&mut tx, uuid, detail_uuid, promotionable_type, &qual)
                .await?;

            let rule_tag_count = rule_tags.len();

            span.record("rule_tag_count", tracing::field::display(rule_tag_count));

            let taggables = self.tags.resolve_taggable_tags(&mut tx, &rule_tags).await?;

            self.tags.create_taggables(&mut tx, &taggables).await?;
        }

        tx.commit().await?;

        info!(promotion_uuid = %uuid, detail_uuid = %detail_uuid, "updated promotion");

        Ok(())
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

    async fn update_promotion(
        &self,
        tenant: TenantUuid,
        uuid: PromotionUuid,
        update: PromotionUpdate,
    ) -> Result<(), PromotionsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use smallvec::smallvec;
    use sqlx::{Row, postgres::PgRow};
    use testresult::TestResult;
    use uuid::Uuid;

    use crate::{
        domain::promotions::{
            data::{
                NewPromotion, PromotionUpdate,
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

    #[tokio::test]
    async fn create_promotion_inserts_rows_as_expected() -> TestResult {
        let ctx = TestContext::new().await;

        let promotion = ctx
            .promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
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
             FROM direct_discount_promotions
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

    #[tokio::test]
    async fn update_promotion_succeeds() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = PromotionUuid::new();

        ctx.promotions
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

        ctx.promotions
            .update_promotion(
                ctx.tenant_uuid,
                uuid,
                PromotionUpdate::DirectDiscount {
                    budgets: Budgets {
                        redemptions: Some(50),
                        monetary: Some(5_000),
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 10 },
                    qualification: None,
                },
            )
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn update_promotion_not_found_returns_not_found() -> TestResult {
        let ctx = TestContext::new().await;

        let result = ctx
            .promotions
            .update_promotion(
                ctx.tenant_uuid,
                PromotionUuid::new(),
                PromotionUpdate::DirectDiscount {
                    budgets: Budgets {
                        redemptions: None,
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 10 },
                    qualification: None,
                },
            )
            .await;

        assert!(
            matches!(result, Err(PromotionsServiceError::NotFound)),
            "expected NotFound, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn update_promotion_creates_new_detail_row() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = PromotionUuid::new();

        ctx.promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid,
                    budgets: Budgets {
                        redemptions: Some(100),
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 20 },
                    qualification: None,
                },
            )
            .await?;

        ctx.promotions
            .update_promotion(
                ctx.tenant_uuid,
                uuid,
                PromotionUpdate::DirectDiscount {
                    budgets: Budgets {
                        redemptions: Some(50),
                        monetary: None,
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 200 },
                    qualification: None,
                },
            )
            .await?;

        let rows: Vec<(bool,)> = sqlx::query_as(
            "SELECT upper_inf(valid_period)
             FROM direct_discount_promotions
             WHERE promotion_uuid = $1
             ORDER BY created_at",
        )
        .bind(uuid.into_uuid())
        .fetch_all(ctx.db.pool())
        .await?;

        assert_eq!(rows.len(), 2, "expected two detail rows after update");
        assert!(!rows[0].0, "first row should be closed");
        assert!(rows[1].0, "second row should be open");

        let current: ExpectedDetail = sqlx::query_as(
            "SELECT
               redemption_budget,
               monetary_budget,
               discount_kind::text AS kind,
               discount_percentage,
               discount_amount
             FROM direct_discount_promotions
             WHERE promotion_uuid = $1
               AND upper_inf(valid_period)",
        )
        .bind(uuid.into_uuid())
        .fetch_one(ctx.db.pool())
        .await?;

        assert_eq!(
            current,
            ExpectedDetail {
                redemption_budget: Some(50),
                monetary_budget: None,
                kind: "amount_off".to_string(),
                discount_percentage: None,
                discount_amount: Some(200),
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn update_promotion_creates_versioned_qualification_and_tags() -> TestResult {
        let ctx = TestContext::new().await;
        let uuid = PromotionUuid::new();

        ctx.promotions
            .create_promotion(
                ctx.tenant_uuid,
                NewPromotion::DirectDiscount {
                    uuid,
                    budgets: Budgets {
                        redemptions: Some(100),
                        monetary: None,
                    },
                    discount: SimpleDiscount::PercentageOff { percentage: 20 },
                    qualification: None,
                },
            )
            .await?;

        ctx.promotions
            .update_promotion(
                ctx.tenant_uuid,
                uuid,
                PromotionUpdate::DirectDiscount {
                    budgets: Budgets {
                        redemptions: Some(50),
                        monetary: None,
                    },
                    discount: SimpleDiscount::FixedAmountOff { amount: 200 },
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
                },
            )
            .await?;

        let current_detail_uuid: Uuid = sqlx::query_scalar(
            "SELECT uuid
             FROM direct_discount_promotions
             WHERE promotion_uuid = $1
               AND upper_inf(valid_period)",
        )
        .bind(uuid.into_uuid())
        .fetch_one(ctx.db.pool())
        .await?;

        let qualification_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT promotionable_type::text
             FROM qualifications
             WHERE promotionable_uuid = $1",
        )
        .bind(current_detail_uuid)
        .fetch_all(ctx.db.pool())
        .await?;

        assert_eq!(qualification_rows, vec![("direct".to_string(),)]);

        let old_detail_qualification_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM qualifications
             WHERE promotionable_uuid = $1",
        )
        .bind(uuid.into_uuid())
        .fetch_one(ctx.db.pool())
        .await?;

        assert_eq!(old_detail_qualification_count, 0);

        let rule_tags: Vec<(String, String)> = sqlx::query_as(
            "SELECT qr.kind::text, t.name
             FROM qualification_rules qr
             JOIN qualifications q ON q.uuid = qr.qualification_uuid
             JOIN taggables tg
               ON tg.taggable_uuid = qr.uuid
              AND tg.taggable_type = 'qualification_rule'
             JOIN tags t ON t.uuid = tg.tag_uuid
             WHERE q.promotionable_uuid = $1
             ORDER BY qr.kind::text, t.name",
        )
        .bind(current_detail_uuid)
        .fetch_all(ctx.db.pool())
        .await?;

        assert_eq!(
            rule_tags,
            vec![
                ("has_any".to_string(), "included".to_string()),
                ("has_none".to_string(), "excluded".to_string()),
            ]
        );

        Ok(())
    }
}
