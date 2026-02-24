//! Promotion Qualification Data

use crate::domain::promotions::records::{QualificationRuleUuid, QualificationUuid};

/// Qualification Context Data
#[derive(Debug, Default, Clone, PartialEq)]
pub enum QualificationContext {
    #[default]
    Primary,
    Group,
}

/// Qualification Operator Data
#[derive(Debug, Clone, PartialEq)]
pub enum QualificationOp {
    And,
    Or,
}

/// New Qualification Rule Data
#[derive(Debug, Clone, PartialEq)]
pub enum NewQualificationRule {
    HasAll {
        uuid: QualificationRuleUuid,
        tags: Vec<String>,
    },
    HasAny {
        uuid: QualificationRuleUuid,
        tags: Vec<String>,
    },
    HasNone {
        uuid: QualificationRuleUuid,
        tags: Vec<String>,
    },
    Group {
        uuid: QualificationRuleUuid,
        qualification: NewQualification,
    },
}

/// New Qualification Data
#[derive(Debug, Clone, PartialEq)]
pub struct NewQualification {
    pub uuid: QualificationUuid,
    pub context: QualificationContext,
    pub rules: Vec<NewQualificationRule>,
}
