//! Auth service.

use async_trait::async_trait;
use jiff::Timestamp;
use mockall::automock;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::openbao::OpenBaoClient,
    auth::{
        ApiTokenMetadata, ApiTokenVersion, AuthServiceError, IssuedApiToken, NewApiToken,
        build_verifier_input, format_api_token, generate_api_token_secret, parse_api_token,
        repository::PgAuthRepository,
    },
    domain::tenants::records::TenantUuid,
};

#[derive(Debug, Clone)]
pub struct PgAuthService {
    repository: PgAuthRepository,
    openbao: OpenBaoClient,
}

impl PgAuthService {
    #[must_use]
    pub fn new(pool: PgPool, openbao: OpenBaoClient) -> Self {
        Self {
            repository: PgAuthRepository::new(pool),
            openbao,
        }
    }

    /// Issue a new API token for the given tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if HMAC computation or database insertion fails.
    pub async fn issue_api_token(
        &self,
        tenant_uuid: Uuid,
        expires_at: Option<Timestamp>,
    ) -> Result<IssuedApiToken, AuthServiceError> {
        let token_uuid = Uuid::now_v7();
        let version = ApiTokenVersion::V1;
        let secret = generate_api_token_secret();
        let token = format_api_token(token_uuid, version, &secret);

        let verifier_input =
            build_verifier_input(&token_uuid, version, &tenant_uuid.into(), &secret);

        let token_hash = self.openbao.hmac(&verifier_input).await?;

        let metadata = self
            .repository
            .create_api_token(&NewApiToken {
                uuid: token_uuid,
                tenant_uuid: tenant_uuid.into(),
                version,
                token_hash,
                expires_at,
            })
            .await
            .map_err(AuthServiceError::from)?;

        Ok(IssuedApiToken { token, metadata })
    }

    /// List all tokens for the given tenant.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn list_api_tokens(
        &self,
        tenant_uuid: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, AuthServiceError> {
        self.repository
            .list_api_tokens_by_tenant(tenant_uuid.into())
            .await
            .map_err(AuthServiceError::from)
    }

    /// Revoke a token by UUID. Returns `true` if the token was active.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn revoke_api_token(&self, token_uuid: Uuid) -> Result<bool, AuthServiceError> {
        self.repository
            .revoke_api_token(token_uuid)
            .await
            .map(|record| record.is_some())
            .map_err(AuthServiceError::from)
    }
}

#[async_trait]
impl AuthService for PgAuthService {
    async fn authenticate_bearer(
        &self,
        bearer_token: &str,
    ) -> Result<TenantUuid, AuthServiceError> {
        let parsed_token = parse_api_token(bearer_token).map_err(|_| AuthServiceError::NotFound)?;

        let token = self
            .repository
            .find_active_api_token_by_uuid(parsed_token.token_uuid, parsed_token.version)
            .await
            .map_err(AuthServiceError::from)?
            .ok_or(AuthServiceError::NotFound)?;

        if token.version != parsed_token.version {
            return Err(AuthServiceError::NotFound);
        }

        let verifier_input = build_verifier_input(
            &parsed_token.token_uuid,
            parsed_token.version,
            &token.tenant_uuid,
            &parsed_token.secret,
        );

        let valid = self
            .openbao
            .verify(&verifier_input, &token.token_hash)
            .await?;

        if !valid {
            return Err(AuthServiceError::NotFound);
        }

        // Best-effort metadata update; auth success should not depend on this write.
        let _touch_result = self
            .repository
            .touch_api_token_last_used(parsed_token.token_uuid)
            .await;

        Ok(token.tenant_uuid)
    }
}

#[automock]
#[async_trait]
pub trait AuthService: Send + Sync {
    async fn authenticate_bearer(&self, bearer_token: &str)
    -> Result<TenantUuid, AuthServiceError>;
}
