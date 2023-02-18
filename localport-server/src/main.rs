use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use server::Server;

mod cert;
mod server;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_websockets=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let server = Server::new(3000, "");
    server.run().await
}
