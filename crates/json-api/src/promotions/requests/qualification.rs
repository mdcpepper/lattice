//! Promotion Qualification Requests

use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use lattice_app::domain::promotions::data::qualification::{
    Qualification, QualificationContext, QualificationOp, QualificationRule,
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
    #[serde(default)]
    pub context: QualificationContextRequest,

    pub op: QualificationOpRequest,

    #[serde(default)]
    pub rules: Vec<CreateQualificationRuleRequest>,
}

impl From<CreateQualificationRequest> for Qualification {
    fn from(request: CreateQualificationRequest) -> Self {
        Qualification {
            context: request.context.into(),
            op: request.op.into(),
            rules: request.rules.into_iter().map(Into::into).collect(),
        }
    }
}

/// Create Qualification Rule Request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CreateQualificationRuleRequest {
    HasAll {
        tags: SmallVec<[String; 3]>,
    },
    HasAny {
        tags: SmallVec<[String; 3]>,
    },

    HasNone {
        tags: SmallVec<[String; 3]>,
    },

    Group {
        qualification: CreateQualificationRequest,
    },
}

impl From<CreateQualificationRuleRequest> for QualificationRule {
    fn from(request: CreateQualificationRuleRequest) -> Self {
        match request {
            CreateQualificationRuleRequest::HasAll { tags } => QualificationRule::HasAll { tags },
            CreateQualificationRuleRequest::HasAny { tags } => QualificationRule::HasAny { tags },
            CreateQualificationRuleRequest::HasNone { tags } => QualificationRule::HasNone { tags },
            CreateQualificationRuleRequest::Group { qualification } => QualificationRule::Group {
                qualification: qualification.into(),
            },
        }
    }
}
