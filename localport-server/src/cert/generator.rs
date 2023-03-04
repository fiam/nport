use std::time::Duration;

use acme_lib::{create_p384_key, persist::MemoryPersist, Certificate, Directory, DirectoryUrl};
use tokio::time::{sleep, Instant};
use tracing::debug;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts},
    TokioAsyncResolver,
};

use super::dns::Updater;

pub struct Generator {
    email: String,
    domain: String,
    staging: bool,
    updater: Box<dyn Updater + Send + Sync>,
}

impl Generator {
    pub fn new<T: AsRef<str>>(
        email: T,
        domain: T,
        staging: bool,
        updater: Box<dyn Updater + Send + Sync>,
    ) -> Self {
        Self {
            email: email.as_ref().to_string(),
            domain: domain.as_ref().to_string(),
            staging,
            updater,
        }
    }

    async fn txt_lookup(&self, record: &str) -> anyhow::Result<Vec<String>> {
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default()).unwrap();

        let response = resolver.txt_lookup(record).await?;
        Ok(response.into_iter().map(|r| r.to_string()).collect())
    }

    pub async fn request(&self) -> Result<Certificate, anyhow::Error> {
        let url = if self.staging {
            DirectoryUrl::LetsEncryptStaging
        } else {
            DirectoryUrl::LetsEncrypt
        };

        let suffix = if self.domain.starts_with("*.") {
            &self.domain[2..self.domain.len()]
        } else {
            &self.domain
        };
        let record = format!("_acme-challenge.{suffix}");

        let persist = MemoryPersist::new();
        let dir = Directory::from_url(persist, url)?;

        let account = dir.account(&self.email)?;

        let mut ord_new = account.new_order(&self.domain, &[])?;

        let ord_csr = loop {
            if let Some(ord_csr) = ord_new.confirm_validations() {
                break ord_csr;
            }

            let auths = ord_new.authorizations()?;

            let challenge = auths[0].dns_challenge();

            let token = challenge.dns_proof();
            debug!(token, "DNS challenge");
            self.updater.update(&record, &token).await?;

            // Wait until the TXT lookup is propagated
            let wait_start = Instant::now();
            loop {
                match self.txt_lookup(&record).await {
                    Ok(records) => {
                        if records.contains(&token) {
                            // Wait a bit after confirmation, otherwise we get
                            // random failures due to propagation delay
                            sleep(Duration::from_millis(5000)).await;
                            break;
                        }
                    }
                    Err(error) => {
                        debug!(error=?error, record=record, "DNS lookup")
                    }
                }
                if wait_start.elapsed().as_secs() > 90 {
                    return Err(anyhow::anyhow!("DNS update timed out"));
                }
                sleep(Duration::from_millis(5000)).await;
            }
            debug!("DNS challenge took {:?}", wait_start.elapsed());

            challenge.validate(15000)?;

            // Update the state against the ACME API.
            ord_new.refresh()?;
        };

        let pkey_pri = create_p384_key();

        // Submit the CSR. This causes the ACME provider to enter a
        // state of "processing" that must be polled until the
        // certificate is either issued or rejected. Again we poll
        // for the status change.
        let ord_cert = ord_csr.finalize_pkey(pkey_pri, 5000)?;

        Ok(ord_cert.download_and_save_cert()?)
    }
}
