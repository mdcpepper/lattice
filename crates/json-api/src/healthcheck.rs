//! Lattice JSON API Healthcheck Handler

use salvo::{oapi::ToSchema, prelude::*};
use serde::{Deserialize, Serialize};

/// Healthcheck response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
}

/// Healthcheck handler
///
/// Returns service health status
#[endpoint(tags("health"), summary = "Health check endpoint")]
pub(crate) async fn handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use salvo::{
        prelude::*,
        test::{ResponseExt, TestClient},
    };
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn test_healthcheck() -> TestResult {
        let router = Router::new().push(Router::with_path("healthcheck").get(handler));

        let response: HealthResponse = TestClient::get("http://example.com/healthcheck")
            .send(&Service::new(router))
            .await
            .take_json()
            .await?;

        assert_eq!(response.status, "ok");

        Ok(())
    }
}
