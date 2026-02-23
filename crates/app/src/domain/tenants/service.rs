//! Tenants service.

use async_trait::async_trait;
use mockall::automock;
use sqlx::PgPool;

use crate::domain::tenants::{
    data::NewTenant, errors::TenantsServiceError, records::TenantRecord,
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
    async fn create_tenant(&self, tenant: NewTenant) -> Result<TenantRecord, TenantsServiceError> {
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
    async fn create_tenant(&self, tenant: NewTenant) -> Result<TenantRecord, TenantsServiceError>;
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use testresult::TestResult;

    use crate::{domain::tenants::records::TenantUuid, test::TestContext};

    use super::*;

    #[tokio::test]
    async fn create_tenant_returns_correct_uuid_and_name() -> TestResult {
        let ctx = TestContext::new().await;
        let svc = PgTenantsService::new(ctx.db.pool().clone());

        let uuid = TenantUuid::new();

        let tenant = svc
            .create_tenant(NewTenant {
                uuid,
                name: "Acme Corp".to_string(),
            })
            .await?;

        assert_eq!(tenant.uuid, uuid);
        assert_eq!(tenant.name, "Acme Corp");
        assert!(tenant.deleted_at.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn create_tenant_timestamps_are_set() -> TestResult {
        let ctx = TestContext::new().await;
        let svc = PgTenantsService::new(ctx.db.pool().clone());

        let before = Timestamp::now();

        let tenant = svc
            .create_tenant(NewTenant {
                uuid: TenantUuid::new(),
                name: "Timestamp Test".to_string(),
            })
            .await?;

        let after = Timestamp::now();

        assert!(tenant.created_at >= before);
        assert!(tenant.created_at <= after);

        Ok(())
    }

    #[tokio::test]
    async fn create_tenant_duplicate_uuid_returns_already_exists() -> TestResult {
        let ctx = TestContext::new().await;
        let svc = PgTenantsService::new(ctx.db.pool().clone());

        let uuid = TenantUuid::new();

        svc.create_tenant(NewTenant {
            uuid,
            name: "First".to_string(),
        })
        .await?;

        let result = svc
            .create_tenant(NewTenant {
                uuid,
                name: "Second".to_string(),
            })
            .await;

        assert!(
            matches!(result, Err(TenantsServiceError::AlreadyExists)),
            "expected AlreadyExists, got {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn create_tenant_duplicate_name_succeeds() -> TestResult {
        let ctx = TestContext::new().await;
        let svc = PgTenantsService::new(ctx.db.pool().clone());

        // Name has no uniqueness constraint — two tenants may share a name
        svc.create_tenant(NewTenant {
            uuid: TenantUuid::new(),
            name: "Shared Name".to_string(),
        })
        .await?;

        svc.create_tenant(NewTenant {
            uuid: TenantUuid::new(),
            name: "Shared Name".to_string(),
        })
        .await?;

        Ok(())
    }

    #[tokio::test]
    async fn create_tenant_multiple_tenants_are_independent() -> TestResult {
        let ctx = TestContext::new().await;
        let svc = PgTenantsService::new(ctx.db.pool().clone());

        let uuid_a = TenantUuid::new();
        let uuid_b = TenantUuid::new();

        let tenant_a = svc
            .create_tenant(NewTenant {
                uuid: uuid_a,
                name: "Tenant A".to_string(),
            })
            .await?;

        let tenant_b = svc
            .create_tenant(NewTenant {
                uuid: uuid_b,
                name: "Tenant B".to_string(),
            })
            .await?;

        assert_eq!(tenant_a.uuid, uuid_a);
        assert_eq!(tenant_b.uuid, uuid_b);
        assert_ne!(tenant_a.uuid, tenant_b.uuid);
        assert_ne!(tenant_a.name, tenant_b.name);

        Ok(())
    }
}
