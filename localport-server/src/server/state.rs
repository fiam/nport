use std::sync::Arc;

use tokio::sync::Mutex;

use super::registry::Registry;

#[derive(Default)]
pub struct AppState {
    registry: Registry,
}

impl AppState {
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
