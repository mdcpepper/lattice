//! Auth data models.

use jiff::Timestamp;
use uuid::Uuid;

use crate::{auth::ApiTokenVersion, tenants::models::TenantUuid};

/// API token data used during bearer authentication.
#[derive(Debug, Clone)]
pub(crate) struct ActiveApiToken {
    /// Tenant that owns this API token.
    pub tenant_uuid: TenantUuid,

    /// Token format/hash version.
    pub version: ApiTokenVersion,

    /// OpenBao HMAC verifier for the token secret material.
    pub token_hash: String,
}

/// API token metadata persisted in storage.
#[derive(Debug, Clone)]
pub struct ApiTokenMetadata {
    pub uuid: Uuid,
    pub tenant_uuid: TenantUuid,
    pub version: ApiTokenVersion,
    pub created_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub expires_at: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
}

/// New API token persistence payload.
#[derive(Debug, Clone)]
pub struct NewApiToken {
    pub uuid: Uuid,
    pub tenant_uuid: TenantUuid,
    pub version: ApiTokenVersion,
    pub token_hash: String,
    pub expires_at: Option<Timestamp>,
}

/// API token issuance result with one-time raw token.
#[derive(Debug, Clone)]
pub struct IssuedApiToken {
    pub token: String,
    pub metadata: ApiTokenMetadata,
}
