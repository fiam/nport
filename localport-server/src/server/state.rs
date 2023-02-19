use std::sync::Arc;

use super::registry::Registry;

#[derive(Default)]
pub struct AppState {
    registry: Arc<Registry>,
    tls: bool,
}

impl AppState {
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn with_tls(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            tls: true,
        }
    }
}

pub type SharedState = Arc<AppState>;
