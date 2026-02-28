//! Process-global observability runtime settings.

use std::sync::OnceLock;

use crate::config::ServerConfig;

const DEFAULT_SLOW_REQUEST_THRESHOLD_MS: u64 = 1_000;
const DEFAULT_OTEL_PARENT_PROPAGATION_ENABLED: bool = false;

static SLOW_REQUEST_THRESHOLD_MS: OnceLock<u64> = OnceLock::new();
static OTEL_PARENT_PROPAGATION_ENABLED: OnceLock<bool> = OnceLock::new();

pub(super) fn apply_runtime_config(config: &ServerConfig) {
    _ = SLOW_REQUEST_THRESHOLD_MS.set(config.observability.slow_request_threshold_ms);
    _ = OTEL_PARENT_PROPAGATION_ENABLED.set(config.observability.otel_parent_propagation_enabled);
}

pub(super) fn slow_request_threshold_ms() -> u64 {
    *SLOW_REQUEST_THRESHOLD_MS
        .get()
        .unwrap_or(&DEFAULT_SLOW_REQUEST_THRESHOLD_MS)
}

pub(super) fn otel_parent_propagation_enabled() -> bool {
    *OTEL_PARENT_PROPAGATION_ENABLED
        .get()
        .unwrap_or(&DEFAULT_OTEL_PARENT_PROPAGATION_ENABLED)
}
