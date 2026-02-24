//! Promotion Qualification Requests

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QualificationContextRequest {
    #[default]
    Primary,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QualificationOpRequest {
    And,
    Or,
}

/// A boolean expression node. Nesting is expressed by having `Group` rules contain
/// another `Qualification`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualificationRequest {
    pub uuid: Uuid,

    #[serde(default)]
    pub context: QualificationContextRequest,

    pub op: QualificationOpRequest,

    /// Ordered rules. Array order is your `sort_order`.
    #[serde(default)]
    pub rules: Vec<QualificationRuleRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QualificationRuleRequest {
    HasAll {
        uuid: Uuid,
        tags: Vec<String>,
    },
    HasAny {
        uuid: Uuid,
        tags: Vec<String>,
    },

    HasNone {
        uuid: Uuid,
        tags: Vec<String>,
    },

    /// Nested node
    Group {
        uuid: Uuid,
        qualification: QualificationRequest,
    },
}
