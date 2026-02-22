//! Tenants service.

use async_trait::async_trait;
use mockall::automock;
use sqlx::PgPool;

use crate::tenants::{
    errors::TenantsServiceError,
    models::{NewTenant, Tenant},
    repository::PgTenantsRepository,
};

#[derive(Debug, Clone)]
pub struct PgTenantsService {
    repository: PgTenantsRepository,
}

impl PgTenantsService {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            repository: PgTenantsRepository::new(pool),
        }
    }
}

#[async_trait]
impl TenantsService for PgTenantsService {
    async fn create_tenant(&self, tenant: NewTenant) -> Result<Tenant, TenantsServiceError> {
        self.repository
            .create_tenant(tenant)
            .await
            .map_err(Into::into)
    }
}

#[automock]
#[async_trait]
/// Tenant persistence operations.
pub trait TenantsService: Send + Sync {
    /// Creates a new tenant.
    async fn create_tenant(&self, tenant: NewTenant) -> Result<Tenant, TenantsServiceError>;
}
