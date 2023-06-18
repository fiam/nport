use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::{Host, State},
    http::{
        uri::{Authority, Scheme},
        Request, Uri,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use rustls::ServerConfig;
use tokio::sync::{oneshot, Mutex};
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use state::SharedState;

mod builder;
mod client;
mod handlers;
mod hostname;
mod msghandlers;
mod port_server;
mod registry;
mod state;

pub use builder::Builder;

use crate::cert;

use self::state::AppState;

static DEFAULT_HTTPS_PORT: u16 = 443;

async fn to_tls_middleware<B>(
    State(state): State<SharedState>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    if state.has_tls() && !state.via_tls() {
        let mut parts = request.uri().clone().into_parts();
        parts.scheme = Some(Scheme::HTTPS);
        let authority_str = if state.https_port() == DEFAULT_HTTPS_PORT {
            state.hostname().to_string()
        } else {
            format!("{}:{}", state.hostname(), state.https_port())
        };
        if let Ok(authority) = authority_str.parse::<Authority>() {
            parts.authority = Some(authority);
            if let Ok(uri) = Uri::from_parts(parts) {
                return axum::response::Redirect::permanent(&uri.to_string()).into_response();
            }
        }
    }

    next.run(request).await
}

pub struct Server {
    http_port: u16,
    https_port: u16,
    domain: String,
    cert_store: Option<Arc<cert::Store>>,
    client_request_timeout_secs: u16,
    http_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    https_shutdown: Mutex<Option<oneshot::Sender<()>>>,
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
            .route("/", get(handlers::home))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                to_tls_middleware,
            ))
            .with_state(state.clone());

        let forwarding_router = Router::new()
            .route("/", any(handlers::forward))
            .route("/*path", any(handlers::forward))
            .with_state(state.clone());

        let public_hostname = self.public_hostname().to_string();

        let chooser = |Host(hostname): Host, request: Request<Body>| async move {
            // Split port
            let host = if let Some(parts) = hostname.rsplit_once(':') {
                parts.0
            } else {
                &hostname
            };
            tracing::debug!(host, public_hostname, "routing request");
            let router = if host == public_hostname {
                tracing::debug!(host, "routing request to main");
                host_router
            } else {
                tracing::debug!(host, "routing request to forwarding");
                forwarding_router
            };
            router.oneshot(request).await
        };

        Router::new()
            .route("/*path", any(chooser.clone()))
            .route("/", any(chooser))
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
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        *self.http_shutdown.lock().await = Some(shutdown_tx);
        tracing::debug!("listening for HTTP on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async move {
                shutdown_rx.await.ok();
            })
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
        let state = Arc::new(AppState::new(
            self.http_port,
            self.https_port,
            &self.domain,
            self.public_hostname(),
            self.client_request_timeout_secs,
        ));

        let http = self.run_http(state.clone());
        let https = self.run_https(state.clone());

        let (http_result, https_result) = tokio::join!(http, https);
        http_result.unwrap();
        https_result.unwrap();
    }

    /// Stops the server. Notice that this only implemented for HTTP for the time being
    pub async fn stop(&self) {
        if let Some(shutdown) = self.http_shutdown.lock().await.take() {
            shutdown.send(()).ok();
        }
        if let Some(shutdown) = self.https_shutdown.lock().await.take() {
            shutdown.send(()).ok();
        }
    }
}
