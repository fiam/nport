use dotenvy::dotenv;
use envconfig::Envconfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = nport_server::server::Config::init_from_env().unwrap();
    let server = config.server().await.expect("invalid Server configuration");
    server.run().await
}
