//! Depot helper extensions.

use std::any::Any;

use salvo::prelude::{Depot, StatusError};

use lattice_app::tenants::models::TenantUuid;

const TENANT_UUID_KEY: &str = "tenant_uuid";

/// Helpers for mapping depot extraction failures to HTTP errors.
pub(crate) trait DepotExt {
    fn obtain_or_500<T: Any + Send + Sync>(&self) -> Result<&T, StatusError>;
    fn tenant_uuid_or_401(&self) -> Result<TenantUuid, StatusError>;
    fn insert_tenant_uuid(&mut self, tenant_uuid: TenantUuid);
}

impl DepotExt for Depot {
    fn obtain_or_500<T: Any + Send + Sync>(&self) -> Result<&T, StatusError> {
        self.obtain::<T>()
            .map_err(|_ignored| StatusError::internal_server_error())
    }

    fn tenant_uuid_or_401(&self) -> Result<TenantUuid, StatusError> {
        self.get::<TenantUuid>(TENANT_UUID_KEY)
            .ok()
            .copied()
            .ok_or_else(StatusError::unauthorized)
    }

    fn insert_tenant_uuid(&mut self, tenant_uuid: TenantUuid) {
        self.insert(TENANT_UUID_KEY, tenant_uuid);
    }
}
