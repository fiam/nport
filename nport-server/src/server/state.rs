use std::sync::Arc;

use super::registry::Registry;

pub struct AppState {
    registry: Arc<Registry>,
    http_port: u16,
    https_port: u16,
    hostname: String,
    via_tls: bool,
}

impl AppState {
    pub fn new(http_port: u16, https_port: u16, hostname: &str) -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            http_port,
            https_port,
            hostname: hostname.to_string(),
            via_tls: false,
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn with_tls(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            http_port: self.http_port,
            https_port: self.https_port,
            hostname: self.hostname.clone(),
            via_tls: true,
        }
    }

    pub fn via_tls(&self) -> bool {
        self.via_tls
    }

    pub fn has_tls(&self) -> bool {
        self.https_port > 0
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn https_port(&self) -> u16 {
        self.https_port
    }
}

pub type SharedState = Arc<AppState>;
