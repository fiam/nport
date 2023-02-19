use std::{
    collections::HashMap,
    fs,
    ops::Add,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use async_trait::async_trait;
use rustls::{
    server::{ClientHello, ResolvesServerCert},
    sign::{self, CertifiedKey},
    Certificate, PrivateKey,
};
use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use x509_parser::prelude::{FromDer, X509Certificate};

use super::generator::Generator;

static MINIMUM_CERT_LOAD_DURATION: Duration = Duration::days(1);
static MINIMUM_CERT_RENEW_DURATION: Duration = Duration::days(30);

#[async_trait]
pub trait CertStore: ResolvesServerCert + Send + Sync {
    async fn update(&self);
}

#[derive(Serialize, Deserialize)]
pub struct StoredCert {
    pub domain: String,
    pub private_key_der: Vec<u8>,
    pub certificate_der: Vec<u8>,
}

impl StoredCert {
    pub fn validate(&self) -> Result<()> {
        if self.domain.is_empty() {
            Err(anyhow::anyhow!("empty domain"))
        } else if self.private_key_der.is_empty() {
            Err(anyhow::anyhow!("empty private key"))
        } else if self.certificate_der.is_empty() {
            Err(anyhow::anyhow!("empty certificate"))
        } else {
            Ok(())
        }
    }
}

pub struct Store {
    certs: RwLock<HashMap<String, Arc<CertifiedKey>>>,
    dir: String,
    domain: String,
    generator: Arc<Generator>,
    renewal_lock: Mutex<()>,
}

impl Store {
    pub fn new(dir: &str, domain: &str, generator: Arc<Generator>) -> Self {
        Self {
            certs: RwLock::new(HashMap::new()),
            dir: dir.to_string(),
            domain: domain.to_string(),
            generator,
            renewal_lock: Mutex::new(()),
        }
    }

    pub fn update_cert(&self, domain: &str, cert: &acme_lib::Certificate) {
        self.add(domain, cert.private_key_der(), cert.certificate_der());
        if let Err(err) = self.store_cert(domain, cert) {
            error!(error=?err, domain=domain, "storing certificate");
        }
    }

    pub fn add(&self, domain: &str, private_key_der: Vec<u8>, certificate_der: Vec<u8>) {
        let certificates = vec![Certificate(certificate_der)];
        let key = sign::any_supported_type(&PrivateKey(private_key_der)).unwrap();
        let certified = Arc::new(CertifiedKey::new(certificates, key));
        self.certs
            .write()
            .unwrap()
            .insert(domain.to_string(), certified);
    }

    fn cert_filename(&self, domain: &str) -> String {
        domain.replace('*', "STAR")
    }

    fn store_cert(&self, domain: &str, cert: &acme_lib::Certificate) -> Result<()> {
        let stored = StoredCert {
            domain: domain.to_string(),
            private_key_der: cert.private_key_der(),
            certificate_der: cert.certificate_der(),
        };
        let data = serde_json::to_string(&stored)?;
        let dir_path = std::path::Path::new(&self.dir);
        if !dir_path.exists() {
            fs::create_dir_all(dir_path)?;
        }
        let path = dir_path.join(self.cert_filename(domain));
        fs::write(path, data)?;
        Ok(())
    }

    fn should_renew(&self) -> bool {
        let certs = self.certs.read().unwrap();
        let cert = certs.get(&self.domain);
        match cert {
            Some(cert) => match cert_expiration(cert.cert[0].as_ref()) {
                Ok(expiration) => {
                    if expiration.lt(&OffsetDateTime::now_utc().add(MINIMUM_CERT_RENEW_DURATION)) {
                        debug!(domain = self.domain, "trying to renew certificate");
                        return true;
                    }
                    false
                }
                Err(err) => {
                    warn!(error=?err, "error parsing in-memory cert");
                    true
                }
            },
            None => true,
        }
    }

    pub async fn load(&self) -> Result<()> {
        let dir_path = std::path::Path::new(&self.dir);
        let files = fs::read_dir(dir_path)?;
        for p in files {
            let fp = p?;
            let data = fs::read_to_string(fp.path())?;
            let cert = serde_json::from_str::<StoredCert>(&data)?;
            cert.validate()?;
            let expiration = cert_expiration(&cert.certificate_der)?;
            if expiration.lt(&OffsetDateTime::now_utc().add(MINIMUM_CERT_LOAD_DURATION)) {
                warn!(
                    domain = cert.domain,
                    "certificate expires in less than {} days, ignoring",
                    MINIMUM_CERT_LOAD_DURATION.whole_days()
                );
                continue;
            }
            self.add(&cert.domain, cert.private_key_der, cert.certificate_der);
            debug!(domain = cert.domain, "loaded certificate");
        }
        Ok(())
    }

    pub async fn renew(&self) -> Result<()> {
        if self.should_renew() {
            let _guard = self.renewal_lock.lock().await;
            let cert = self.generator.request().await?;
            self.update_cert(&self.domain, &cert);
            info!(domain = self.domain, "renewed cert");
        }
        Ok(())
    }

    pub async fn update(&self) {
        if let Err(err) = self.renew().await {
            error!(errror=?err, domain = self.domain,"renewing certificate");
        }
    }
}

impl ResolvesServerCert for Store {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        if let Some(name) = client_hello.server_name() {
            let top_level = format!(
                "*.{}",
                name.split('.').skip(1).collect::<Vec<_>>().join(".")
            );
            {
                let certs = self.certs.read().unwrap();
                if let Some(cert) = certs.get(&top_level) {
                    // TODO: Refresh if needed
                    return Some(cert.clone());
                }
            }
        }
        None
    }
}

fn cert_expiration(der: &[u8]) -> Result<OffsetDateTime> {
    let mut rem = der;
    let mut expiration = Date::MAX
        .midnight()
        .assume_offset(time::macros::offset!(+0));
    loop {
        let (extra, x509) = X509Certificate::from_der(rem)?;
        let validity = x509.validity();
        if validity.not_after.to_datetime().lt(&expiration) {
            expiration = validity.not_after.to_datetime();
        }
        if extra.is_empty() {
            return Ok(expiration);
        }
        rem = extra;
    }
}
