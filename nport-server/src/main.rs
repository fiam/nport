use std::env;

use dotenvy::dotenv;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut builder = nport_server::server::Builder::default();
    if let Ok(val) = env::var("HTTP_PORT") {
        let port = val.parse::<u16>().expect("invalid HTTP port");
        builder = builder.http_port(port);
    }
    if let Ok(val) = env::var("HTTPS_PORT") {
        let port = val.parse::<u16>().expect("invalid HTTPS port");
        builder = builder.https_port(port);
    }
    if let Ok(val) = env::var("DOMAIN") {
        builder = builder.domain(val);
    }
    if let Ok(val) = env::var("CERTS_DIR") {
        builder = builder.certs_dir(val);
    }

    if let Ok(val) = env::var("ACME_EMAIL") {
        builder = builder.acme_email(val);
    }

    if let Ok(val) = env::var("ACME_DOMAIN") {
        builder = builder.acme_domain(val);
    }

    if let Ok(val) = env::var("ACME_STAGING") {
        let staging = val.parse::<bool>().expect("invalid ACME_STAGING value");
        builder = builder.acme_staging(staging);
    }

    if let Ok(val) = env::var("CLOUDFLARE_ZONE_ID") {
        builder = builder.cloudflare_zone_id(val);
    }

    if let Ok(val) = env::var("CLOUDFLARE_API_TOKEN") {
        builder = builder.cloudflare_api_token(val);
    }
    let server = builder
        .server()
        .await
        .expect("invalid Server configuration");
    server.run().await
}
