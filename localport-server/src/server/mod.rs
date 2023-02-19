use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::Host,
    http::Request,
    routing::{any, get},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use rustls::ServerConfig;
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use state::SharedState;

mod builder;
mod client;
mod handlers;
mod registry;
mod state;

pub use builder::Builder;

use crate::cert;

pub struct Server {
    http_port: u16,
    https_port: u16,
    domain: String,
    cert_store: Option<Arc<cert::Store>>,
}

impl Server {
    fn public_hostname(&self) -> &str {
        if self.domain.is_empty() {
            "127.0.0.1"
        } else {
            &self.domain
        }
    }

    fn build_app(&self, state: SharedState) -> Router {
        let host_router = Router::new()
            .route("/v1/connect", get(handlers::websocket))
            .with_state(state.clone());

        let forwarding_router = Router::new()
            .route("/*path", any(handlers::forward))
            .with_state(state.clone());

        let public_hostname = self.public_hostname().to_string();
        Router::new()
            .route(
                "/*path",
                any(|Host(hostname): Host, request: Request<Body>| async move {
                    // Split port
                    let host = if let Some(parts) = hostname.rsplit_once(':') {
                        parts.0
                    } else {
                        &hostname
                    };
                    tracing::trace!(host, "routing request");
                    let router = if host == public_hostname {
                        host_router
                    } else {
                        forwarding_router
                    };
                    router.oneshot(request).await
                }),
            )
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::default().include_headers(true)),
            )
            .with_state(state)
    }

    async fn run_http(&self, state: SharedState) -> anyhow::Result<()> {
        if self.http_port == 0 {
            return Ok(());
        }
        let app = self.build_app(state);
        let addr = SocketAddr::from(([127, 0, 0, 1], self.http_port));
        tracing::debug!("listening for HTTP on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|err| err.into())
    }

    async fn run_https(&self, state: SharedState) -> anyhow::Result<()> {
        if self.https_port == 0 {
            return Ok(());
        }
        let cert_store = self
            .cert_store
            .clone()
            .expect("HTTPS port without cert store");
        let state = Arc::new(state.with_tls());
        let app = self.build_app(state);

        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_cert_resolver(cert_store.clone());

        let addr = SocketAddr::from(([127, 0, 0, 1], self.https_port));
        tracing::debug!("listening for HTTPS on {}", addr);
        let server = axum_server::bind_rustls(addr, RustlsConfig::from_config(config.into()))
            .serve(app.into_make_service());
        let updater = tokio::spawn(async move {
            loop {
                cert_store.update().await;
                tokio::time::sleep(std::time::Duration::from_millis(3600 * 1000)).await;
            }
        });

        let (server_result, updater_result) = tokio::join!(server, updater);
        updater_result?;
        server_result?;
        Ok(())
    }

    pub async fn run(&self) {
        let state = SharedState::default();

        let http = self.run_http(state.clone());
        let https = self.run_https(state.clone());

        let (http_result, https_result) = tokio::join!(http, https);
        http_result.unwrap();
        https_result.unwrap();
    }
}
