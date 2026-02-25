//! Promotions Records

use jiff::Timestamp;

use crate::{domain::tags::Taggable, uuids::TypedUuid};

/// Promotion UUID
pub type PromotionUuid = TypedUuid<PromotionRecord>;

/// Promotion Record
#[derive(Debug, Clone)]
pub struct PromotionRecord {
    pub uuid: PromotionUuid,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub deleted_at: Option<Timestamp>,
}

/// Qualification UUID
pub type QualificationUuid = TypedUuid<QualificationRecord>;

/// Qualification Record
#[derive(Debug, Clone)]
pub struct QualificationRecord {}

/// Qualification Rule UUID
pub type QualificationRuleUuid = TypedUuid<QualificationRuleRecord>;

/// Qualification Rule Record
#[derive(Debug, Clone)]
pub struct QualificationRuleRecord {}

impl Taggable for QualificationRuleRecord {
    fn type_as_str() -> &'static str {
        "qualification_rule"
    }
}
