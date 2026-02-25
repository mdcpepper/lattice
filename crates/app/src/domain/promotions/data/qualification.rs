//! Promotion Qualification Data

use smallvec::SmallVec;

/// Qualification Context Data
#[derive(Debug, Default, Clone, PartialEq)]
pub enum QualificationContext {
    #[default]
    Primary,
    Group,
}

impl QualificationContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualificationContext::Primary => "primary",
            QualificationContext::Group => "group",
        }
    }
}

/// Qualification Operator Data
#[derive(Debug, Clone, PartialEq)]
pub enum QualificationOp {
    And,
    Or,
}

impl QualificationOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            QualificationOp::And => "and",
            QualificationOp::Or => "or",
        }
    }
}

/// New Qualification Rule Data
#[derive(Debug, Clone, PartialEq)]
pub enum QualificationRule {
    HasAll { tags: SmallVec<[String; 3]> },
    HasAny { tags: SmallVec<[String; 3]> },
    HasNone { tags: SmallVec<[String; 3]> },
    Group { qualification: Qualification },
}

impl QualificationRule {
    pub fn type_as_str(&self) -> &'static str {
        match self {
            QualificationRule::HasAll { .. } => "has_all",
            QualificationRule::HasAny { .. } => "has_any",
            QualificationRule::HasNone { .. } => "has_none",
            QualificationRule::Group { .. } => "group",
        }
    }
}

/// New Qualification Data
#[derive(Debug, Clone, PartialEq)]
pub struct Qualification {
    pub context: QualificationContext,
    pub op: QualificationOp,
    pub rules: Vec<QualificationRule>,
}
