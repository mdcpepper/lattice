//! Tenant Data

use crate::domain::tenants::records::TenantUuid;

/// New Tenant Data
#[derive(Debug, Clone, PartialEq)]
pub struct NewTenant {
    /// UUID to assign to the tenant row.
    pub uuid: TenantUuid,

    /// Tenant name to persist.
    pub name: String,
}
