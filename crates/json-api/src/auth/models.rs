//! Auth data models.

use uuid::Uuid;

use crate::tenants::models::TenantUuid;

/// Represents an API token with associated metadata.
#[derive(Debug, Clone)]
pub(crate) struct ApiToken {
    /// The unique identifier of the API token.
    #[expect(dead_code, reason = "kept for full token record shape")]
    pub uuid: Uuid,

    /// The unique identifier of the tenant associated with the API token.
    pub tenant_uuid: TenantUuid,

    /// The hashed value of the API token.
    #[expect(dead_code, reason = "kept for full token record shape")]
    pub token_hash: String,
}
