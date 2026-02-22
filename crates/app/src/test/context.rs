//! Test context for service-level integration tests.

use sqlx::{Connection, PgConnection, PgPool, query};
use uuid::Uuid;

use crate::{
    database::Db,
    domain::{
        carts::PgCartsService,
        products::PgProductsService,
        tenants::{
            PgTenantsService, TenantsService,
            models::{NewTenant, TenantUuid},
        },
    },
};

use super::db::TestDb;

/// Name of the non-superuser app role used for RLS testing.
const APP_ROLE: &str = "lattice_app_test";
const APP_ROLE_PASSWORD: &str = "lattice_app_test_pass";

pub struct TestContext {
    pub db: TestDb,
    pub tenant_uuid: TenantUuid,
    pub products: PgProductsService,
    pub carts: PgCartsService,
}

impl TestContext {
    pub async fn new() -> Self {
        let test_db = TestDb::new().await;

        // Build a non-superuser app pool so RLS policies are enforced.
        // The superuser pool is only used for administrative setup (tenant creation).
        let app_pool = Self::setup_app_pool(&test_db).await;
        let db = Db::new(app_pool);

        let tenant_uuid = Uuid::now_v7();

        PgTenantsService::new(test_db.pool().clone())
            .create_tenant(NewTenant {
                uuid: tenant_uuid,
                name: "Test Tenant".to_string(),
            })
            .await
            .expect("Failed to create default test tenant");

        Self {
            products: PgProductsService::new(db.clone()),
            carts: PgCartsService::new(db),
            tenant_uuid: TenantUuid::from_uuid(tenant_uuid),
            db: test_db,
        }
    }

    /// Create an additional tenant — useful for RLS isolation tests.
    pub async fn create_tenant(&self, name: &str) -> TenantUuid {
        let uuid = Uuid::now_v7();

        PgTenantsService::new(self.db.pool().clone())
            .create_tenant(NewTenant {
                uuid,
                name: name.to_string(),
            })
            .await
            .expect("Failed to create test tenant");

        TenantUuid::from_uuid(uuid)
    }

    /// Create a non-superuser role (once per server) and return a pool connected as it.
    ///
    /// PostgreSQL superusers bypass RLS even with `FORCE ROW LEVEL SECURITY`, so service
    /// tests that exercise isolation must connect via this restricted role.
    async fn setup_app_pool(test_db: &TestDb) -> PgPool {
        // `superuser_url` points at the test database as the superuser.
        let su_url = &test_db.superuser_url;

        // Derive a base URL pointing at the `postgres` maintenance database for
        // server-level DDL (CREATE ROLE is server-scoped, not database-scoped).
        let postgres_url = su_url.rsplit_once('/').map(|x| x.0).unwrap_or(su_url);
        let postgres_url = format!("{postgres_url}/postgres");

        let mut server_conn = PgConnection::connect(&postgres_url)
            .await
            .expect("Failed to connect to postgres database for role setup");

        // Create the app role. Multiple parallel tests may race here; treat
        // "role already exists" (42710) or the underlying unique violation (23505)
        // as success — the role is present either way.
        let create_result = query(&format!(
            "CREATE ROLE {APP_ROLE} WITH LOGIN PASSWORD '{APP_ROLE_PASSWORD}' \
               NOSUPERUSER NOCREATEDB NOCREATEROLE"
        ))
        .execute(&mut server_conn)
        .await;

        if let Err(sqlx::Error::Database(ref e)) = create_result {
            if !matches!(e.code().as_deref(), Some("42710") | Some("23505")) {
                create_result.expect("Failed to create app role");
            }
        } else {
            create_result.expect("Failed to create app role");
        }

        // Grant CONNECT on the test database.
        query(&format!(
            "GRANT CONNECT ON DATABASE \"{}\" TO {APP_ROLE}",
            test_db.name
        ))
        .execute(&mut server_conn)
        .await
        .expect("Failed to grant CONNECT on test database");

        server_conn
            .close()
            .await
            .expect("Failed to close server connection");

        // Within the test database, grant schema and table privileges.
        let mut db_conn = PgConnection::connect(su_url)
            .await
            .expect("Failed to connect to test database for privilege setup");

        for stmt in [
            format!("GRANT USAGE ON SCHEMA public TO {APP_ROLE}"),
            format!(
                "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {APP_ROLE}"
            ),
            format!("GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO {APP_ROLE}"),
        ] {
            query(&stmt)
                .execute(&mut db_conn)
                .await
                .expect("Failed to grant table privileges to app role");
        }

        db_conn
            .close()
            .await
            .expect("Failed to close db connection");

        // Connect as the non-superuser role.
        let app_url = su_url.replacen(
            "lattice_test:lattice_test_password",
            &format!("{APP_ROLE}:{APP_ROLE_PASSWORD}"),
            1,
        );

        PgPool::connect(&app_url)
            .await
            .expect("Failed to create app pool")
    }
}
