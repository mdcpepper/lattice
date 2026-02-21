//! Tenant Models
//!
use jiff::Timestamp;
use uuid::Uuid;

use crate::uuids::TypedUuid;

pub type TenantUuid = TypedUuid<Tenant>;

pub type TennantUuid = TenantUuid;

/// Tenant Model
#[derive(Debug, Clone)]
pub struct Tenant {
    /// Unique tenant identifier.
    pub uuid: Uuid,

    /// Human-readable tenant name.
    pub name: String,

    /// Tenant creation timestamp.
    pub created_at: Timestamp,

    /// Last update timestamp.
    pub updated_at: Timestamp,

    /// Soft-delete timestamp when deleted.
    pub deleted_at: Option<Timestamp>,
}

/// New Tenant Model
#[derive(Debug, Clone, PartialEq)]
pub struct NewTenant {
    /// UUID to assign to the tenant row.
    pub uuid: Uuid,

    /// Tenant name to persist.
    pub name: String,

    /// UUID to assign to the API token row.
    pub token_uuid: Uuid,

    /// SHA-256 hash of the raw API token.
    pub token_hash: String,
}
