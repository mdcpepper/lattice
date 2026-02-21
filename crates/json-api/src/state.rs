//! State

use std::sync::Arc;

use lattice_app::context::AppContext;

#[derive(Clone)]
pub(crate) struct State {
    pub(crate) app: AppContext,
}

impl State {
    #[must_use]
    pub(crate) fn new(app: AppContext) -> Self {
        Self { app }
    }

    #[must_use]
    pub(crate) fn from_app_context(app: AppContext) -> Arc<Self> {
        Arc::new(Self::new(app))
    }
}
