//! Qualifications Repository

use smallvec::SmallVec;
use sqlx::{Postgres, Transaction, query};
use tracing::debug;

use crate::domain::promotions::{
    data::qualification::{Qualification, QualificationRule},
    records::{DirectDiscountDetailUuid, PromotionUuid, QualificationRuleUuid, QualificationUuid},
};

const CREATE_QUALIFICATION_SQL: &str = include_str!("../sql/create_qualification.sql");
const CREATE_QUALIFICATION_RULE_SQL: &str = include_str!("../sql/create_qualification_rule.sql");

type RuleTag = (QualificationRuleUuid, SmallVec<[String; 3]>);
type RuleTags = SmallVec<[RuleTag; 5]>;

#[derive(Debug, Clone, Default)]
pub(crate) struct PgQualificationsRepository;

impl PgQualificationsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    #[tracing::instrument(
        name = "promotions.qualifications_repository.create_qualifications",
        skip(self, tx, qualification),
        fields(
            promotion_uuid = %promotion_uuid,
            promotionable_uuid = %promotionable_uuid,
            promotionable_type = %promotionable_type,
            rules_count = qualification.rules.len()
        ),
        err
    )]
    pub(crate) async fn create_qualifications(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        promotion_uuid: PromotionUuid,
        promotionable_uuid: DirectDiscountDetailUuid,
        promotionable_type: &'static str,
        qualification: &Qualification,
    ) -> Result<RuleTags, sqlx::Error> {
        let rule_tags = Box::pin(insert_qualification(
            tx,
            promotion_uuid,
            promotionable_uuid,
            promotionable_type,
            qualification,
            None,
        ))
        .await?;

        debug!(
            promotion_uuid = %promotion_uuid,
            promotionable_uuid = %promotionable_uuid,
            promotionable_type = %promotionable_type,
            rule_tags_count = rule_tags.len(),
            "created qualifications"
        );

        Ok(rule_tags)
    }
}

#[tracing::instrument(
    name = "promotions.qualifications_repository.insert_qualification",
    skip(tx, qualification),
    fields(
        promotion_uuid = %promotion_uuid,
        promotionable_uuid = %promotionable_uuid,
        promotionable_type = %promotionable_type,
        qualification_context = %qualification.context.as_str(),
        qualification_op = %qualification.op.as_str(),
        rules_count = qualification.rules.len(),
        has_parent = tracing::field::Empty
    ),
    err
)]
async fn insert_qualification(
    tx: &mut Transaction<'_, Postgres>,
    promotion_uuid: PromotionUuid,
    promotionable_uuid: DirectDiscountDetailUuid,
    promotionable_type: &'static str,
    qualification: &Qualification,
    parent_uuid: Option<QualificationUuid>,
) -> Result<RuleTags, sqlx::Error> {
    tracing::Span::current().record("has_parent", tracing::field::display(parent_uuid.is_some()));

    let qualification_uuid = QualificationUuid::new();

    query(CREATE_QUALIFICATION_SQL)
        .bind(qualification_uuid.into_uuid())
        .bind(promotion_uuid.into_uuid())
        .bind(promotionable_uuid.into_uuid())
        .bind(qualification.context.as_str())
        .bind(qualification.op.as_str())
        .bind(parent_uuid.map(QualificationUuid::into_uuid))
        .bind(promotionable_type)
        .execute(&mut **tx)
        .await?;

    let mut rule_tags = RuleTags::new();

    for rule in &qualification.rules {
        match rule {
            QualificationRule::Group {
                qualification: nested,
            } => {
                rule_tags.extend(
                    Box::pin(insert_qualification(
                        tx,
                        promotion_uuid,
                        promotionable_uuid,
                        promotionable_type,
                        nested,
                        Some(qualification_uuid),
                    ))
                    .await?,
                );
            }
            QualificationRule::HasAll { tags }
            | QualificationRule::HasAny { tags }
            | QualificationRule::HasNone { tags } => {
                rule_tags
                    .push(insert_leaf_rule_with_tags(tx, qualification_uuid, rule, tags).await?);
            }
        }
    }

    debug!(
        promotion_uuid = %promotion_uuid,
        qualification_uuid = %qualification_uuid,
        rules_count = qualification.rules.len(),
        "inserted qualification"
    );

    Ok(rule_tags)
}

#[tracing::instrument(
    name = "promotions.qualifications_repository.insert_leaf_rule_with_tags",
    skip(tx, tags),
    fields(
        qualification_uuid = %qualification_uuid,
        rule_type = %rule.type_as_str(),
        tags_count = tags.len()
    ),
    err
)]
async fn insert_leaf_rule_with_tags(
    tx: &mut Transaction<'_, Postgres>,
    qualification_uuid: QualificationUuid,
    rule: &QualificationRule,
    tags: &SmallVec<[String; 3]>,
) -> Result<RuleTag, sqlx::Error> {
    let rule_uuid = QualificationRuleUuid::new();

    insert_qualification_rule(tx, qualification_uuid, rule_uuid, rule).await?;

    debug!(
        qualification_uuid = %qualification_uuid,
        rule_uuid = %rule_uuid,
        tags_count = tags.len(),
        "inserted qualification leaf rule"
    );

    Ok((rule_uuid, tags.clone()))
}

#[tracing::instrument(
    name = "promotions.qualifications_repository.insert_qualification_rule",
    skip(tx),
    fields(
        qualification_uuid = %qualification_uuid,
        rule_uuid = %rule_uuid,
        rule_type = %rule.type_as_str()
    ),
    err
)]
async fn insert_qualification_rule(
    tx: &mut Transaction<'_, Postgres>,
    qualification_uuid: QualificationUuid,
    rule_uuid: QualificationRuleUuid,
    rule: &QualificationRule,
) -> Result<(), sqlx::Error> {
    query(CREATE_QUALIFICATION_RULE_SQL)
        .bind(rule_uuid.into_uuid())
        .bind(qualification_uuid.into_uuid())
        .bind(rule.type_as_str())
        .execute(&mut **tx)
        .await?;

    debug!(
        qualification_uuid = %qualification_uuid,
        rule_uuid = %rule_uuid,
        "inserted qualification rule"
    );

    Ok(())
}
