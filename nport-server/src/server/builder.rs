use std::{net::IpAddr, str::FromStr, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;

use crate::cert::{CloudflareUpdater, Generator, Store};

use super::Server;

const DEFAULT_CLIENT_REQUEST_TIMEOUT_SECS: u16 = 30;

#[derive(Debug, Default)]
pub struct Builder {
    production: bool,
    bind_addr: String,
    http_port: u16,
    https_port: u16,
    public_https_port: u16,
    domain: String,
    certs_dir: String,
    acme_email: String,
    acme_persist_dir: String,
    acme_domain: String,
    acme_staging: bool,
    cloudflare_zone_id: String,
    cloudflare_api_token: String,
    client_request_timeout_secs: u16,
}

impl Builder {
    pub fn production(mut self, production: bool) -> Self {
        self.production = production;
        self
    }

    pub fn bind_addr(mut self, addr: &str) -> Self {
        self.bind_addr = addr.to_string();
        self
    }

    pub fn http_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    pub fn https_port(mut self, port: u16) -> Self {
        self.https_port = port;
        self
    }

    pub fn public_https_port(mut self, port: u16) -> Self {
        self.public_https_port = port;
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

    pub fn acme_persist_dir<T: AsRef<str>>(mut self, dir: T) -> Self {
        self.acme_persist_dir = dir.as_ref().to_string();
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
        if self.production {
            self.production_check()?;
        }
        let cert_store = self.cert_store().await?;
        let client_request_timeout_secs = if self.client_request_timeout_secs > 0 {
            self.client_request_timeout_secs
        } else {
            DEFAULT_CLIENT_REQUEST_TIMEOUT_SECS
        };
        let addr_string = if self.bind_addr.is_empty() {
            "127.0.0.1"
        } else {
            &self.bind_addr
        };
        let addr = IpAddr::from_str(addr_string)?;
        Ok(Server {
            bind_addr: addr,
            http_port: self.http_port,
            https_port: self.https_port,
            public_https_port: self.public_https_port,
            domain: self.domain,
            cert_store,
            client_request_timeout_secs,
            http_shutdown: Mutex::new(None),
            https_shutdown: Mutex::new(None),
        })
    }

    fn has_acme(&self) -> bool {
        !self.acme_email.is_empty()
            && !self.acme_persist_dir.is_empty()
            && !self.acme_domain.is_empty()
    }

    fn has_cloudflare(&self) -> bool {
        !self.cloudflare_zone_id.is_empty() && !self.cloudflare_api_token.is_empty()
    }

    fn enable_cert_store(&self) -> bool {
        if !self.has_acme() {
            tracing::debug!("no ACME config found, disabling cert store");
            return false;
        }
        if self.certs_dir.is_empty() {
            tracing::debug!("no certs dir found, disabling cert store");
            return false;
        }

        if !self.has_cloudflare() {
            tracing::debug!("no cloudflare config found, disabling cert store");
        }
        true
    }

    async fn cert_store(&self) -> Result<Option<Arc<Store>>> {
        if self.enable_cert_store() {
            let updater = Box::new(CloudflareUpdater::new(
                &self.cloudflare_api_token,
                &self.cloudflare_zone_id,
            ));
            let generator = Generator::new(
                &self.acme_email,
                &self.acme_persist_dir,
                self.acme_staging,
                updater,
            );
            let store = Store::new(&self.certs_dir, &self.acme_domain, Arc::new(generator));
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

    fn production_check(&self) -> anyhow::Result<()> {
        println!("CHECK {:?}", &self);
        if self.http_port == 0 {
            return Err(anyhow::anyhow!("missing http_port"));
        }
        if self.https_port == 0 {
            return Err(anyhow::anyhow!("missing https_port"));
        }
        if self.domain.is_empty() {
            return Err(anyhow::anyhow!("missing domain"));
        }
        if self.certs_dir.is_empty() {
            return Err(anyhow::anyhow!("missing certs_dir"));
        }
        if self.acme_email.is_empty() {
            return Err(anyhow::anyhow!("missing acme_email"));
        }
        if self.acme_domain.is_empty() {
            return Err(anyhow::anyhow!("missing acme_domain"));
        }
        if self.acme_staging {
            return Err(anyhow::anyhow!("acme_staging is true"));
        }
        if self.cloudflare_zone_id.is_empty() {
            return Err(anyhow::anyhow!("missing cloudflare_zone_id"));
        }
        if self.cloudflare_api_token.is_empty() {
            return Err(anyhow::anyhow!("missing cloudflare_api_token"));
        }
        Ok(())
    }
}
