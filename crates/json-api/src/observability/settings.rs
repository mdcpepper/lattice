//! Process-global observability runtime settings.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::config::ServerConfig;

const DEFAULT_SLOW_REQUEST_THRESHOLD_MS: u64 = 1_000;

static SLOW_REQUEST_THRESHOLD_MS: AtomicU64 = AtomicU64::new(DEFAULT_SLOW_REQUEST_THRESHOLD_MS);
static OTEL_PARENT_PROPAGATION_ENABLED: AtomicBool = AtomicBool::new(false);

pub(super) fn apply_runtime_config(config: &ServerConfig) {
    SLOW_REQUEST_THRESHOLD_MS.store(config.slow_request_threshold_ms, Ordering::Relaxed);
    OTEL_PARENT_PROPAGATION_ENABLED.store(config.otel_enabled, Ordering::Relaxed);
}

pub(super) fn slow_request_threshold_ms() -> u64 {
    SLOW_REQUEST_THRESHOLD_MS.load(Ordering::Relaxed)
}

pub(super) fn otel_parent_propagation_enabled() -> bool {
    OTEL_PARENT_PROPAGATION_ENABLED.load(Ordering::Relaxed)
}
