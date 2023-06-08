use std::sync::Arc;

use anyhow::Result;

use crate::cert::{CloudflareUpdater, Generator, Store};

use super::Server;

const DEFAULT_CLIENT_REQUEST_TIMEOUT_SECS: u16 = 30;

#[derive(Default)]
pub struct Builder {
    http_port: u16,
    https_port: u16,
    domain: String,
    certs_dir: String,
    acme_email: String,
    acme_domain: String,
    acme_staging: bool,
    cloudflare_zone_id: String,
    cloudflare_api_token: String,
    client_request_timeout_secs: u16,
}

impl Builder {
    pub fn http_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    pub fn https_port(mut self, port: u16) -> Self {
        self.https_port = port;
        self
    }

    pub fn domain<T: AsRef<str>>(mut self, domain: T) -> Self {
        self.domain = domain.as_ref().to_string();
        self
    }

    pub fn certs_dir<T: AsRef<str>>(mut self, dir: T) -> Self {
        self.certs_dir = dir.as_ref().to_string();
        self
    }

    pub fn acme_email<T: AsRef<str>>(mut self, email: T) -> Self {
        self.acme_email = email.as_ref().to_string();
        self
    }

    pub fn acme_domain<T: AsRef<str>>(mut self, domain: T) -> Self {
        self.acme_domain = domain.as_ref().to_string();
        self
    }

    pub fn acme_staging(mut self, staging: bool) -> Self {
        self.acme_staging = staging;
        self
    }

    pub fn cloudflare_zone_id<T: AsRef<str>>(mut self, zone_id: T) -> Self {
        self.cloudflare_zone_id = zone_id.as_ref().to_string();
        self
    }

    pub fn cloudflare_api_token<T: AsRef<str>>(mut self, token: T) -> Self {
        self.cloudflare_api_token = token.as_ref().to_string();
        self
    }

    pub fn client_request_timeout_secs(mut self, secs: u16) -> Self {
        self.client_request_timeout_secs = secs;
        self
    }

    pub async fn server(self) -> anyhow::Result<Server> {
        let cert_store = self.cert_store().await?;
        let client_request_timeout_secs = if self.client_request_timeout_secs > 0 {
            self.client_request_timeout_secs
        } else {
            DEFAULT_CLIENT_REQUEST_TIMEOUT_SECS
        };
        Ok(Server {
            http_port: self.http_port,
            https_port: self.https_port,
            domain: self.domain,
            cert_store,
            client_request_timeout_secs,
        })
    }

    fn domain_for_acme(&self) -> String {
        if !self.acme_domain.is_empty() {
            self.acme_domain.clone()
        } else if !self.domain.is_empty() {
            format!("*.{}", self.domain)
        } else {
            "".to_string()
        }
    }

    fn has_acme(&self) -> bool {
        !self.acme_email.is_empty() && !self.domain_for_acme().is_empty()
    }

    fn has_cloudflare(&self) -> bool {
        !self.cloudflare_zone_id.is_empty() && !self.cloudflare_api_token.is_empty()
    }

    async fn cert_store(&self) -> Result<Option<Arc<Store>>> {
        if self.has_acme() && !self.certs_dir.is_empty() && self.has_cloudflare() {
            let updater = Box::new(CloudflareUpdater::new(
                &self.cloudflare_api_token,
                &self.cloudflare_zone_id,
            ));
            let generator = Generator::new(
                &self.acme_email,
                &self.domain_for_acme(),
                self.acme_staging,
                updater,
            );
            let store = Store::new(
                &self.certs_dir,
                &self.domain_for_acme(),
                Arc::new(generator),
            );
            store.load().await?;
            Ok(Some(Arc::new(store)))
        } else {
            if self.https_port > 0 {
                return Err(anyhow::anyhow!(
                    "HTTPS port without certificate configuration"
                ));
            }
            Ok(None)
        }
    }
}
