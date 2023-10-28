use std::sync::Arc;

use anyhow::Result;

use super::{
    config::{Hostnames, Listen},
    implementation::Options,
    registry::Registry,
    templates::Templates,
};

pub struct AppState {
    registry: Arc<Registry>,
    listen: Listen,
    hostnames: Hostnames,
    via_tls: bool,
    options: Options,
    templates: Arc<Templates>,
}

impl AppState {
    pub fn new(listen: &Listen, hostnames: &Hostnames, options: &Options) -> Result<Self> {
        let templates = Arc::new(Templates::new()?);
        Ok(Self {
            registry: Arc::new(Registry::default()),
            listen: listen.clone(),
            hostnames: hostnames.clone(),
            via_tls: false,
            options: options.clone(),
            templates,
        })
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
            templates: self.templates.clone(),
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

    pub fn templates(&self) -> &Templates {
        &self.templates
    }
}

pub type SharedState = Arc<AppState>;
