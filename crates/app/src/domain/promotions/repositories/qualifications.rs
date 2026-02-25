//! Qualifications Repository

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
        promotion_uuid: PromotionUuid,
        qualification: &Qualification,
    ) -> Result<(), sqlx::Error> {
        Box::pin(insert_qualification(
            tx,
            promotion_uuid,
            qualification,
            None,
        ))
        .await
    }
}

async fn insert_qualification(
    tx: &mut Transaction<'_, Postgres>,
    promotion_uuid: PromotionUuid,
    qualification: &Qualification,
    parent_qualification_uuid: Option<QualificationUuid>,
) -> Result<(), sqlx::Error> {
    let qual_uuid = QualificationUuid::new();

    query(CREATE_QUALIFICATION_SQL)
        .bind(qual_uuid.into_uuid())
        .bind(promotion_uuid.into_uuid())
        .bind(qualification.context.as_str())
        .bind(qualification.op.as_str())
        .bind(parent_qualification_uuid.map(QualificationUuid::into_uuid))
        .execute(&mut **tx)
        .await?;

    for rule in &qualification.rules {
        match rule {
            QualificationRule::Group {
                qualification: nested,
            } => {
                Box::pin(insert_qualification(
                    tx,
                    promotion_uuid,
                    nested,
                    Some(qual_uuid),
                ))
                .await?;
            }
            _ => {
                insert_qualification_rule(tx, qual_uuid, rule.type_as_str()).await?;
            }
        }
    }

    Ok(())
}

async fn insert_qualification_rule(
    tx: &mut Transaction<'_, Postgres>,
    qualification_uuid: QualificationUuid,
    kind: &'static str,
) -> Result<(), sqlx::Error> {
    query(CREATE_QUALIFICATION_RULE_SQL)
        .bind(QualificationRuleUuid::new().into_uuid())
        .bind(qualification_uuid.into_uuid())
        .bind(kind)
        .execute(&mut **tx)
        .await?;

    Ok(())
}
