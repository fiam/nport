use std::sync::Arc;

use super::{
    config::{Hostnames, Listen},
    implementation::Options,
    registry::Registry,
};

pub struct AppState {
    registry: Arc<Registry>,
    listen: Listen,
    hostnames: Hostnames,
    via_tls: bool,
    options: Options,
}

impl AppState {
    pub fn new(listen: &Listen, hostnames: &Hostnames, options: &Options) -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            listen: listen.clone(),
            hostnames: hostnames.clone(),
            via_tls: false,
            options: options.clone(),
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
            options: self.options.clone(),
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

    pub fn options(&self) -> &Options {
        &self.options
    }
}

pub type SharedState = Arc<AppState>;
