//! Auth middleware.

use std::sync::Arc;

use lattice_app::auth::AuthServiceError;
use salvo::{http::header::AUTHORIZATION, prelude::*};
use tracing::error;

use crate::{extensions::*, state::State};

#[salvo::handler]
pub(crate) async fn handler(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let Some(token) = extract_bearer_token(req) else {
        res.render(StatusError::unauthorized().brief("Missing or invalid Authorization header"));

        return;
    };

    let state = match depot.obtain::<Arc<State>>() {
        Ok(state) => state,
        Err(_error) => {
            res.render(StatusError::internal_server_error());

            return;
        }
    };

    let tenant_uuid = match state.app.auth.authenticate_bearer(token).await {
        Ok(tenant_uuid) => tenant_uuid,
        Err(AuthServiceError::NotFound) => {
            res.render(StatusError::unauthorized().brief("Invalid API token"));

            return;
        }
        Err(AuthServiceError::Sql(source)) => {
            error!("failed to validate api token: {source}");

            res.render(StatusError::internal_server_error());

            return;
        }
        Err(AuthServiceError::Token(source)) => {
            error!("failed to process api token: {source}");

            res.render(StatusError::internal_server_error());

            return;
        }
        Err(AuthServiceError::OpenBao(source)) => {
            error!("OpenBao error during token authentication: {source}");

            res.render(StatusError::internal_server_error());

            return;
        }
    };

    depot.insert_tenant_uuid(tenant_uuid);

    ctrl.call_next(req, depot, res).await;
}

fn extract_bearer_token(req: &Request) -> Option<&str> {
    let value = req.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let mut parts = value.splitn(2, ' ');

    let scheme = parts.next()?;
    let token = parts.next()?.trim();

    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
        return None;
    }

    Some(token)
}

#[cfg(test)]
mod tests {
    use lattice_app::{auth::MockAuthService, tenants::models::TenantUuid};
    use salvo::{
        affix_state::inject,
        test::{ResponseExt, TestClient},
    };
    use testresult::TestResult;
    use uuid::Uuid;

    use crate::test_helpers::state_with_auth;

    use super::*;

    #[salvo::handler]
    async fn echo_tenant(depot: &mut Depot, res: &mut Response) {
        let tenant = depot.tenant_uuid_or_401().ok().map_or_else(
            || "missing".to_string(),
            |uuid: TenantUuid| uuid.to_string(),
        );

        res.render(tenant);
    }

    fn make_service(auth: MockAuthService) -> Service {
        let state = state_with_auth(auth);

        let router = Router::new()
            .hoop(inject(state))
            .hoop(handler)
            .push(Router::new().get(echo_tenant));

        Service::new(router)
    }

    #[tokio::test]
    async fn test_missing_authorization_header_returns_401() -> TestResult {
        let mut auth = MockAuthService::new();

        auth.expect_authenticate_bearer().never();

        let res = TestClient::get("http://example.com")
            .send(&make_service(auth))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::UNAUTHORIZED));

        Ok(())
    }

    #[tokio::test]
    async fn test_non_bearer_authorization_header_returns_401() -> TestResult {
        let mut auth = MockAuthService::new();

        auth.expect_authenticate_bearer().never();

        let res = TestClient::get("http://example.com")
            .add_header(AUTHORIZATION, "Basic abc123", true)
            .send(&make_service(auth))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::UNAUTHORIZED));

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_token_returns_401() -> TestResult {
        let mut auth = MockAuthService::new();

        auth.expect_authenticate_bearer()
            .once()
            .withf(|token| token == "abc123")
            .return_once(|_| Err(AuthServiceError::NotFound));

        let res = TestClient::get("http://example.com")
            .add_header(AUTHORIZATION, "Bearer abc123", true)
            .send(&make_service(auth))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::UNAUTHORIZED));

        Ok(())
    }

    #[tokio::test]
    async fn test_valid_token_injects_tenant_uuid() -> TestResult {
        let tenant = TenantUuid::from_uuid(Uuid::nil());

        let mut auth = MockAuthService::new();

        auth.expect_authenticate_bearer()
            .once()
            .withf(|token| token == "abc123")
            .return_once(move |_| Ok(tenant));

        let mut res = TestClient::get("http://example.com")
            .add_header(AUTHORIZATION, "Bearer abc123", true)
            .send(&make_service(auth))
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));
        assert_eq!(res.take_string().await?, tenant.to_string());

        Ok(())
    }
}
