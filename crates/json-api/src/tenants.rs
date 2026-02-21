//! Tenants

use crate::uuids::TypedUuid;

#[derive(Debug)]
pub(crate) struct Tenant;

pub(crate) type TenantUuid = TypedUuid<Tenant>;

#[expect(dead_code, reason = "temporary backwards-compatible alias")]
pub(crate) type TennantUuid = TenantUuid;
