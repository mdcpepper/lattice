//! Promotion Qualification Requests

use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use lattice_app::domain::promotions::data::qualification::{
    NewQualification, NewQualificationRule, QualificationContext, QualificationOp,
};

/// Qualification Context Request
#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QualificationContextRequest {
    #[default]
    Primary,
    Group,
}

impl From<QualificationContextRequest> for QualificationContext {
    fn from(request: QualificationContextRequest) -> Self {
        match request {
            QualificationContextRequest::Primary => QualificationContext::Primary,
            QualificationContextRequest::Group => QualificationContext::Group,
        }
    }
}

/// Qualification Operation Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QualificationOpRequest {
    And,
    Or,
}

impl From<QualificationOpRequest> for QualificationOp {
    fn from(request: QualificationOpRequest) -> Self {
        match request {
            QualificationOpRequest::And => QualificationOp::And,
            QualificationOpRequest::Or => QualificationOp::Or,
        }
    }
}

/// Create Qualification Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct CreateQualificationRequest {
    pub uuid: Uuid,

    #[serde(default)]
    pub context: QualificationContextRequest,

    pub op: QualificationOpRequest,

    #[serde(default)]
    pub rules: Vec<CreateQualificationRuleRequest>,
}

impl From<CreateQualificationRequest> for NewQualification {
    fn from(request: CreateQualificationRequest) -> Self {
        NewQualification {
            uuid: request.uuid.into(),
            context: request.context.into(),
            rules: request.rules.into_iter().map(Into::into).collect(),
        }
    }
}

/// Create Qualification Rule Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateQualificationRuleRequest {
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

    Group {
        uuid: Uuid,
        qualification: CreateQualificationRequest,
    },
}

impl From<CreateQualificationRuleRequest> for NewQualificationRule {
    fn from(request: CreateQualificationRuleRequest) -> Self {
        match request {
            CreateQualificationRuleRequest::HasAll { uuid, tags } => NewQualificationRule::HasAll {
                uuid: uuid.into(),
                tags,
            },
            CreateQualificationRuleRequest::HasAny { uuid, tags } => NewQualificationRule::HasAny {
                uuid: uuid.into(),
                tags,
            },
            CreateQualificationRuleRequest::HasNone { uuid, tags } => {
                NewQualificationRule::HasNone {
                    uuid: uuid.into(),
                    tags,
                }
            }
            CreateQualificationRuleRequest::Group {
                uuid,
                qualification,
            } => NewQualificationRule::Group {
                uuid: uuid.into(),
                qualification: qualification.into(),
            },
        }
    }
}
