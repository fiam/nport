mod cert;
mod cloudflare;
mod dns;

#[cfg(test)]
mod tests {
    use std::env;

    use tracing_test::traced_test;

    use super::*;

    #[traced_test]
    #[tokio::test]
    async fn test_obtain_certificate() {
        let email = env::var("ACME_EMAIL").unwrap_or_default();
        let domain = env::var("ACME_DOMAIN").unwrap_or_default();
        let zone_id = env::var("CLOUDFLARE_ZONE_ID").unwrap_or_default();
        let token = env::var("CLOUDFLARE_API_TOKEN").unwrap_or_default();
        if email.is_empty() || domain.is_empty() || zone_id.is_empty() || token.is_empty() {
            println!("missing data for DNS cert test, skipping");
            return;
        }
        let updater = Box::new(cloudflare::Updater::new(&token, &zone_id));

        let cert = cert::Cert::new(email, domain, true, updater);
        cert.request().await.unwrap();
    }
}
