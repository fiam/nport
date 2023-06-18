use std::sync::Arc;

use super::registry::Registry;

pub struct AppState {
    registry: Arc<Registry>,
    http_port: u16,
    https_port: u16,
    public_https_port: u16,
    domain: String,
    hostname: String,
    via_tls: bool,
    client_request_timeout_secs: u16,
}

impl AppState {
    pub fn new(
        http_port: u16,
        https_port: u16,
        public_https_port: u16,
        domain: &str,
        hostname: &str,
        client_request_timeout_secs: u16,
    ) -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            http_port,
            https_port,
            public_https_port,
            domain: domain.to_string(),
            hostname: hostname.to_string(),
            via_tls: false,
            client_request_timeout_secs,
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
            public_https_port: self.public_https_port,
            domain: self.domain.clone(),
            hostname: self.hostname.clone(),
            via_tls: true,
            client_request_timeout_secs: self.client_request_timeout_secs,
        }
    }

    pub fn via_tls(&self) -> bool {
        self.via_tls
    }

    pub fn has_tls(&self) -> bool {
        self.https_port > 0
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn https_port(&self) -> u16 {
        self.https_port
    }

    pub fn public_https_port(&self) -> u16 {
        self.public_https_port
    }

    pub fn client_request_timeout_secs(&self) -> u16 {
        self.client_request_timeout_secs
    }
}

pub type SharedState = Arc<AppState>;
