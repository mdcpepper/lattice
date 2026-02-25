//! Qualifications Repository

use smallvec::SmallVec;
use sqlx::{Postgres, Transaction, query};

use crate::domain::promotions::{
    data::qualification::{Qualification, QualificationRule},
    records::{PromotionUuid, QualificationRuleUuid, QualificationUuid},
};

const CREATE_QUALIFICATION_SQL: &str = include_str!("../sql/create_qualification.sql");
const CREATE_QUALIFICATION_RULE_SQL: &str = include_str!("../sql/create_qualification_rule.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgQualificationsRepository;

impl PgQualificationsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn create_qualifications(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        promotion: PromotionUuid,
        qualification: &Qualification,
    ) -> Result<SmallVec<[(QualificationRuleUuid, SmallVec<[String; 3]>); 5]>, sqlx::Error> {
        Box::pin(insert_qualification(tx, promotion, qualification, None)).await
    }
}

async fn insert_qualification(
    tx: &mut Transaction<'_, Postgres>,
    promotion: PromotionUuid,
    qualification: &Qualification,
    parent_uuid: Option<QualificationUuid>,
) -> Result<SmallVec<[(QualificationRuleUuid, SmallVec<[String; 3]>); 5]>, sqlx::Error> {
    let uuid = QualificationUuid::new();

    query(CREATE_QUALIFICATION_SQL)
        .bind(uuid.into_uuid())
        .bind(promotion.into_uuid())
        .bind(qualification.context.as_str())
        .bind(qualification.op.as_str())
        .bind(parent_uuid.map(QualificationUuid::into_uuid))
        .execute(&mut **tx)
        .await?;

    let mut rule_tags = SmallVec::new();

    for rule in &qualification.rules {
        match rule {
            QualificationRule::Group {
                qualification: nested,
            } => {
                let nested_tag_pairs =
                    Box::pin(insert_qualification(tx, promotion, nested, Some(uuid))).await?;

                rule_tags.extend(nested_tag_pairs);
            }
            _ => {
                let rule_uuid = QualificationRuleUuid::new();

                insert_qualification_rule(tx, uuid, rule_uuid, rule).await?;

                rule_tags.push((
                    rule_uuid,
                    match rule {
                        QualificationRule::HasAll { tags }
                        | QualificationRule::HasAny { tags }
                        | QualificationRule::HasNone { tags } => tags.clone(),
                        QualificationRule::Group { .. } => SmallVec::new(),
                    },
                ));
            }
        }
    }

    Ok(rule_tags)
}

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

    Ok(())
}
