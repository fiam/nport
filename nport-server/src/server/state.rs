use std::sync::Arc;

use super::{
    config::{Hostnames, Listen},
    registry::Registry,
};

pub struct AppState {
    registry: Arc<Registry>,
    listen: Listen,
    hostnames: Hostnames,
    via_tls: bool,
    client_request_timeout_secs: u16,
}

impl AppState {
    pub fn new(listen: &Listen, hostnames: &Hostnames, client_request_timeout_secs: u16) -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            listen: listen.clone(),
            hostnames: hostnames.clone(),
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
            listen: self.listen.clone(),
            hostnames: self.hostnames.clone(),
            via_tls: true,
            client_request_timeout_secs: self.client_request_timeout_secs,
        }
    }

    pub fn via_tls(&self) -> bool {
        self.via_tls
    }

    pub fn has_tls(&self) -> bool {
        self.https_port() > 0
    }

    pub fn hostnames(&self) -> &Hostnames {
        &self.hostnames
    }

    pub fn https_port(&self) -> u16 {
        self.listen.https()
    }

    pub fn client_request_timeout_secs(&self) -> u16 {
        self.client_request_timeout_secs
    }
}

pub type SharedState = Arc<AppState>;
