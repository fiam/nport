use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use acme_lib::{create_p384_key, persist::FilePersist, Certificate, Directory, DirectoryUrl};
use tokio::time::{sleep, Instant};
use tracing::debug;
use trust_dns_resolver::{
    config::{ResolverConfig, ResolverOpts},
    TokioAsyncResolver,
};

use super::dns::Updater;

pub struct Generator {
    email: String,
    persist_dir: PathBuf,
    staging: bool,
    updater: Box<dyn Updater + Send + Sync>,
}

impl Generator {
    pub fn new<S: AsRef<str>, P: AsRef<Path>>(
        email: S,
        persist_dir: P,
        staging: bool,
        updater: Box<dyn Updater + Send + Sync>,
    ) -> Self {
        Self {
            email: email.as_ref().to_string(),
            persist_dir: persist_dir.as_ref().to_path_buf(),
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

    pub async fn request(&self, domain: &str) -> Result<Certificate, anyhow::Error> {
        tracing::debug!(domain, "requesting cert");
        let url = if self.staging {
            DirectoryUrl::LetsEncryptStaging
        } else {
            DirectoryUrl::LetsEncrypt
        };

        let suffix = if domain.starts_with("*.") {
            &domain[2..domain.len()]
        } else {
            domain
        };
        let record = format!("_acme-challenge.{suffix}");

        let persist = FilePersist::new(&self.persist_dir);
        let dir = Directory::from_url(persist, url)?;

        let account = dir.account(&self.email)?;

        let mut ord_new = account.new_order(domain, &[])?;

        let ord_csr = loop {
            if let Some(ord_csr) = ord_new.confirm_validations() {
                break ord_csr;
            }

            let auths = ord_new.authorizations()?;

            let challenge = auths[0].dns_challenge();

            let token = challenge.dns_proof();
            debug!(domain, token, "DNS challenge");
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
