//! Pyroscope profiling lifecycle and request tag management.

use std::sync::{Arc, Mutex, OnceLock};

use pyroscope::{
    PyroscopeError, Result as PyroscopeResult, ThreadId,
    backend::{BackendConfig, PprofConfig, Tag, ThreadTag, pprof_backend},
    pyroscope::{PyroscopeAgent, PyroscopeAgentBuilder, PyroscopeAgentRunning},
};
use tracing::error;

use crate::config::ServerConfig;

use super::ObservabilityError;

type PyroscopeTagFn =
    dyn Fn(ThreadId, String, String) -> PyroscopeResult<()> + Send + Sync + 'static;

#[derive(Clone)]
struct PyroscopeTagger {
    add: Arc<PyroscopeTagFn>,
    remove: Arc<PyroscopeTagFn>,
}

static PYROSCOPE_TAGGER: Mutex<Option<PyroscopeTagger>> = Mutex::new(None);
static PYROSCOPE_REQUEST_TAGS_ENABLED: OnceLock<bool> = OnceLock::new();
const DEFAULT_PYROSCOPE_REQUEST_TAGS_ENABLED: bool = false;

pub(super) struct Profiling {
    pyroscope_agent: Option<PyroscopeAgent<PyroscopeAgentRunning>>,
}

impl Profiling {
    pub(super) fn init(config: &ServerConfig) -> Result<Self, ObservabilityError> {
        _ = PYROSCOPE_REQUEST_TAGS_ENABLED.set(config.observability.pyroscope_request_tags_enabled);

        let pyroscope_agent = if config.observability.pyroscope_enabled {
            let agent = start_pyroscope_agent(config)?;

            if request_tags_enabled() {
                register_pyroscope_tagger(&agent);
            } else {
                clear_pyroscope_tagger();
            }

            Some(agent)
        } else {
            clear_pyroscope_tagger();

            None
        };

        Ok(Self { pyroscope_agent })
    }

    pub(super) fn shutdown(self) {
        clear_pyroscope_tagger();

        if let Some(agent_running) = self.pyroscope_agent {
            match agent_running.stop() {
                Ok(agent_ready) => agent_ready.shutdown(),
                Err(source) => error!("failed to stop pyroscope agent: {source}"),
            }
        }
    }
}

pub(super) fn add_request_tags(
    thread_id: ThreadId,
    method: &str,
    route: &str,
) -> PyroscopeResult<()> {
    if !request_tags_enabled() {
        return Ok(());
    }

    let tagger = get_tagger()?;

    let Some(tagger) = tagger else {
        return Ok(());
    };

    (tagger.add)(
        thread_id.clone(),
        "http.method".to_owned(),
        method.to_owned(),
    )?;

    (tagger.add)(thread_id, "http.route".to_owned(), route.to_owned())?;

    Ok(())
}

pub(super) fn remove_request_tags(
    thread_id: ThreadId,
    method: &str,
    route: &str,
) -> PyroscopeResult<()> {
    if !request_tags_enabled() {
        return Ok(());
    }

    let tagger = get_tagger()?;

    let Some(tagger) = tagger else {
        return Ok(());
    };

    (tagger.remove)(
        thread_id.clone(),
        "http.method".to_owned(),
        method.to_owned(),
    )?;

    (tagger.remove)(thread_id, "http.route".to_owned(), route.to_owned())?;

    Ok(())
}

fn start_pyroscope_agent(
    config: &ServerConfig,
) -> Result<PyroscopeAgent<PyroscopeAgentRunning>, ObservabilityError> {
    let service_name = config.observability.otel_service_name.as_str();
    let service_version = config.observability.otel_service_version.as_str();
    let deployment_environment = config.observability.otel_deployment_environment.as_str();

    let backend = pprof_backend(
        PprofConfig {
            sample_rate: config.observability.pyroscope_sample_rate,
        },
        BackendConfig::default(),
    );

    let agent = PyroscopeAgentBuilder::new(
        config.observability.pyroscope_server_address.as_str(),
        service_name,
        config.observability.pyroscope_sample_rate,
        "pyroscope-rs",
        service_version,
        backend,
    )
    .tags(vec![
        ("service.name", service_name),
        ("service.version", service_version),
        ("deployment.environment.name", deployment_environment),
    ])
    .build()?;

    Ok(agent.start()?)
}

fn register_pyroscope_tagger(agent: &PyroscopeAgent<PyroscopeAgentRunning>) {
    let backend_for_add = agent.backend.backend.clone();
    let backend_for_remove = agent.backend.backend.clone();

    let add = Arc::new(
        move |thread_id: ThreadId, key: String, value: String| -> PyroscopeResult<()> {
            let backend = backend_for_add.lock()?;
            let backend = backend.as_ref().ok_or(PyroscopeError::BackendImpl)?;

            backend.add_tag(ThreadTag::new(thread_id, Tag::new(key, value)))?;

            Ok(())
        },
    );

    let remove = Arc::new(
        move |thread_id: ThreadId, key: String, value: String| -> PyroscopeResult<()> {
            let backend = backend_for_remove.lock()?;
            let backend = backend.as_ref().ok_or(PyroscopeError::BackendImpl)?;

            backend.remove_tag(ThreadTag::new(thread_id, Tag::new(key, value)))?;

            Ok(())
        },
    );

    set_pyroscope_tagger(add, remove);
}

fn get_tagger() -> PyroscopeResult<Option<PyroscopeTagger>> {
    let state = PYROSCOPE_TAGGER
        .lock()
        .map_err(|_err| PyroscopeError::new("failed to lock pyroscope tagger"))?;

    Ok(state.clone())
}

fn set_pyroscope_tagger(add: Arc<PyroscopeTagFn>, remove: Arc<PyroscopeTagFn>) {
    if let Ok(mut state) = PYROSCOPE_TAGGER.lock() {
        *state = Some(PyroscopeTagger { add, remove });
    }
}

fn clear_pyroscope_tagger() {
    if let Ok(mut state) = PYROSCOPE_TAGGER.lock() {
        *state = None;
    }
}

fn request_tags_enabled() -> bool {
    *PYROSCOPE_REQUEST_TAGS_ENABLED
        .get()
        .unwrap_or(&DEFAULT_PYROSCOPE_REQUEST_TAGS_ENABLED)
}
