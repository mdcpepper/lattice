//! OpenBao Transit client for HMAC operations.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

/// Configuration for connecting to an OpenBao instance.
#[derive(Debug, Clone)]
pub struct OpenBaoConfig {
    /// OpenBao server address, e.g. `"http://localhost:8200"`.
    pub addr: String,

    /// Vault/OpenBao authentication token.
    pub token: String,

    /// Transit key name to use for HMAC operations.
    pub transit_key: String,
}

/// HTTP client for OpenBao Transit HMAC operations.
#[derive(Debug, Clone)]
pub struct OpenBaoClient {
    config: OpenBaoConfig,
    http: Client,
}

impl OpenBaoClient {
    /// Create a new client from the given configuration.
    #[must_use]
    pub fn new(config: OpenBaoConfig) -> Self {
        Self {
            config,
            http: Client::new(),
        }
    }

    /// Compute an HMAC over `input` bytes using the configured transit key.
    ///
    /// Returns the opaque verifier string (e.g. `"vault:v1:..."`) to store in
    /// the database.
    ///
    /// # Errors
    ///
    /// Returns an error on HTTP failure or an unexpected response body.
    pub async fn hmac(&self, input: &[u8]) -> Result<String, OpenBaoError> {
        let url = format!(
            "{}/v1/transit/hmac/{}",
            self.config.addr, self.config.transit_key
        );

        let body = serde_json::json!({ "input": BASE64.encode(input) });

        let response = self
            .http
            .post(&url)
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            return Err(OpenBaoError::UnexpectedResponse(format!(
                "hmac request failed with status {status}: {text}"
            )));
        }

        let parsed: HmacResponse = response.json().await?;

        Ok(parsed.data.hmac)
    }

    /// Verify `input` bytes against a stored `hmac` verifier string.
    ///
    /// Returns `Ok(true)` if valid, `Ok(false)` if the HMAC does not match.
    ///
    /// # Errors
    ///
    /// Returns an error on HTTP failure or an unexpected response body.
    pub async fn verify(&self, input: &[u8], hmac: &str) -> Result<bool, OpenBaoError> {
        let url = format!(
            "{}/v1/transit/verify/{}",
            self.config.addr, self.config.transit_key
        );

        let body = serde_json::json!({
            "input": BASE64.encode(input),
            "hmac": hmac,
        });

        let response = self
            .http
            .post(&url)
            .header("X-Vault-Token", &self.config.token)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            return Err(OpenBaoError::UnexpectedResponse(format!(
                "verify request failed with status {status}: {text}"
            )));
        }

        let parsed: VerifyResponse = response.json().await?;

        Ok(parsed.data.valid)
    }
}

#[derive(Debug, Deserialize)]
struct HmacResponse {
    data: HmacData,
}

#[derive(Debug, Deserialize)]
struct HmacData {
    hmac: String,
}

#[derive(Debug, Deserialize)]
struct VerifyResponse {
    data: VerifyData,
}

#[derive(Debug, Deserialize)]
struct VerifyData {
    valid: bool,
}

/// Errors that can occur when communicating with OpenBao.
#[derive(Debug, Error)]
pub enum OpenBaoError {
    /// An HTTP transport or serialization error occurred.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// OpenBao returned a non-2xx response or unexpected body.
    #[error("unexpected response from OpenBao: {0}")]
    UnexpectedResponse(String),
}
