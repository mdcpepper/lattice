//! Tenant Records

use jiff::Timestamp;

use crate::uuids::TypedUuid;

/// Tenant UUID
pub type TenantUuid = TypedUuid<TenantRecord>;

/// Tenant Record
#[derive(Debug, Clone)]
pub struct TenantRecord {
    /// Unique tenant identifier.
    pub uuid: TenantUuid,

    /// Human-readable tenant name.
    pub name: String,

    /// Tenant creation timestamp.
    pub created_at: Timestamp,

    /// Last update timestamp.
    pub updated_at: Timestamp,

    /// Soft-delete timestamp when deleted.
    pub deleted_at: Option<Timestamp>,
}
