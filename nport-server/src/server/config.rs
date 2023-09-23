use std::{net::IpAddr, str::FromStr, sync::Arc};

use anyhow::Result;
use envconfig::Envconfig;

use crate::cert::{CloudflareUpdater, Generator, Store};

use super::implementation::Server;

#[derive(Debug)]
pub struct StringList(Vec<String>);

impl FromStr for StringList {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StringList(
            s.trim()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        ))
    }
}

#[derive(Debug, Envconfig)]
pub struct Config {
    /// Set to true for production
    #[envconfig(from = "PRODUCTION", default = "false")]
    pub production: bool,
    /// Address to bind to
    #[envconfig(from = "BIND_ADDRESS", default = "127.0.0.1")]
    pub bind_address: String,
    /// Port used for listening for HTTP requests
    #[envconfig(from = "HTTP_PORT", default = "0")]
    pub http_port: u16,
    /// Port used for listening for HTTPS requests
    #[envconfig(from = "HTTPS_PORT", default = "0")]
    pub https_port: u16,
    /// Main domain, used to serve the website from as well
    /// as a parent domain for all the other hosts
    #[envconfig(from = "DOMAIN", default = "")]
    pub domain: String,
    /// Subdomain used for serving the API
    #[envconfig(from = "API_SUBDOMAIN", default = "")]
    pub api_subdomain: String,
    /// Subdomain used for TCP forwardings
    #[envconfig(from = "TCP_SUBDOMAIN", default = "")]
    pub tcp_subdomain: String,
    #[envconfig(from = "SUBDOMAIN_BLOCKLIST", default = "")]
    pub subdomain_blocklist: StringList,
    /// Directory to store the TLS certificates
    #[envconfig(from = "CERTS_DIR", default = "")]
    pub certs_dir: String,
    /// Email used for the ACME account
    #[envconfig(from = "ACME_EMAIL", default = "")]
    pub acme_email: String,
    /// Directory to store ACME persistent data like the account
    /// Omitting this causes too many account creations
    #[envconfig(from = "ACME_PERSIST_DIR", default = "")]
    pub acme_persist_dir: String,
    /// Domain used for obtaining the wildcard certificate
    /// Can be configured as a different domain mainly for testing
    #[envconfig(from = "ACME_DOMAIN", default = "")]
    pub acme_domain: String,
    /// Whether to use the ACME staging server
    #[envconfig(from = "ACME_STAGING", default = "true")]
    pub acme_staging: bool,
    /// Zone ID used to update the DNS record for the ACME DNS challenge
    #[envconfig(from = "CLOUDFLARE_ZONE_ID", default = "")]
    pub cloudflare_zone_id: String,
    /// Token to communicate with the cloudflare API
    #[envconfig(from = "CLOUDFLARE_API_TOKEN", default = "")]
    pub cloudflare_api_token: String,
    /// How many seconds to wait for a client before we reply
    /// to a forwarding requests with 504 status code
    #[envconfig(from = "CLIENT_REQUEST_TIMEOUT_SECS", default = "30")]
    pub client_request_timeout_secs: u16,
}

impl Default for Config {
    fn default() -> Self {
        let hashmap = std::collections::HashMap::new();
        Config::init_from_hashmap(&hashmap).unwrap()
    }
}

impl Config {
    pub async fn server(self) -> anyhow::Result<Server> {
        if self.production {
            self.production_check()?;
        }
        let cert_store = self.cert_store().await?;
        let address = IpAddr::from_str(&self.bind_address)?;
        let listen = Listen::new(address, self.http_port, self.https_port);
        let hostnames = Hostnames::new(
            &self.domain,
            Some(&self.api_subdomain),
            Some(&self.tcp_subdomain),
            Some(&self.subdomain_blocklist.0),
        );
        Ok(Server::new(
            listen,
            hostnames,
            cert_store,
            self.client_request_timeout_secs,
        ))
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

/// Options on how the server listens for connections
#[derive(Clone)]
pub struct Listen {
    /// Local address to bind to
    address: IpAddr,
    /// Port used for HTTP. If zero, it's disabled.
    http: u16,
    /// Port used for HTTPS. If zero, it's disabled
    https: u16,
}

impl Listen {
    pub fn new(address: IpAddr, http: u16, https: u16) -> Self {
        Self {
            address,
            http,
            https,
        }
    }

    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn http(&self) -> u16 {
        self.http
    }

    pub fn https(&self) -> u16 {
        self.https
    }
}

/// Options for the different hostnames used by the server
#[derive(Clone)]
pub struct Hostnames {
    // Main domain used for serving all requests
    // and where the main website is served from
    domain: String,
    // Subdomain used for serve API. Might be empty.
    api: String,
    // Subdomain to use for TCP port forwardings
    tcp: String,

    api_hostname: String,
    tcp_hostname: String,

    // Subdomains that are blocked
    subdomain_blocklist: Vec<String>,
}

fn normalize_subdomain(subdomain: &str) -> String {
    subdomain.to_lowercase()
}

impl Hostnames {
    /// Creates a new Hostnames
    ///
    /// # Arguments
    /// * `domain` -  Main domain used for serving all requests
    /// and where the main website is served from
    /// * `api` - Subdomain used for serve API
    /// * `tcp` - Subdomain to use for TCP port forwardings. Might be empty.
    /// * `blocklist` - Subdomains to disallow for HTTP forwardings
    pub fn new(
        domain: &str,
        api: Option<&str>,
        tcp: Option<&str>,
        blocklist: Option<&[String]>,
    ) -> Self {
        let api = normalize_subdomain(api.unwrap_or_default());
        let tcp = normalize_subdomain(tcp.unwrap_or_default());
        let api_hostname = Hostnames::new_hostname(domain, &api);
        let tcp_hostname = Hostnames::new_hostname(domain, &tcp);
        let subdomain_blocklist = blocklist
            .unwrap_or_default()
            .iter()
            .map(|s| normalize_subdomain(s))
            .collect();
        Self {
            domain: domain.to_string(),
            api,
            tcp,

            api_hostname,
            tcp_hostname,

            subdomain_blocklist,
        }
    }
    fn new_hostname(domain: &str, host: &str) -> String {
        if host.is_empty() {
            domain.to_string()
        } else {
            format!("{}.{}", host, domain)
        }
    }
    pub fn domain(&self) -> &str {
        &self.domain
    }
    pub fn api(&self) -> &str {
        &self.api
    }
    pub fn tcp(&self) -> &str {
        &self.tcp
    }

    /// Hostname used to serve the main website. Apex domain
    /// calculated from self.domain()
    pub fn main_hostname(&self) -> &str {
        &self.domain
    }

    /// Hostname used to serve the API. If empty, the apex
    /// domain is used and both are served from the same router.
    pub fn api_hostname(&self) -> &str {
        &self.api_hostname
    }

    /// Hostname used to serve TCP forwardings.
    pub fn tcp_hostname(&self) -> &str {
        &self.tcp_hostname
    }

    /// Returns true iff the subdomain matches any of the configured
    /// subdomains for api, tcp, etc...
    pub fn is_subdomain_used(&self, subdomain: &str) -> bool {
        let subdomain = normalize_subdomain(subdomain);
        subdomain == self.api || subdomain == self.tcp
    }

    pub fn is_subdomain_blocked(&self, subdomain: &str) -> bool {
        let subdomain = normalize_subdomain(subdomain);
        self.subdomain_blocklist
            .iter()
            .any(|item| item == &subdomain)
    }
}
