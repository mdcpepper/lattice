//! Auth service.

use async_trait::async_trait;
use mockall::automock;
use sqlx::PgPool;

use crate::{
    auth::{AuthServiceError, repository::PgAuthRepository},
    tenants::models::TenantUuid,
};

#[derive(Debug, Clone)]
pub struct PgAuthService {
    repository: PgAuthRepository,
}

impl PgAuthService {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            repository: PgAuthRepository::new(pool),
        }
    }
}

#[async_trait]
impl AuthService for PgAuthService {
    async fn find_tenant_by_token_hash(&self, hash: &str) -> Result<TenantUuid, AuthServiceError> {
        self.repository
            .find_tenant_by_token_hash(hash)
            .await
            .map_err(AuthServiceError::from)?
            .ok_or(AuthServiceError::NotFound)
    }
}

#[automock]
#[async_trait]
pub trait AuthService: Send + Sync {
    async fn find_tenant_by_token_hash(&self, hash: &str) -> Result<TenantUuid, AuthServiceError>;
}
