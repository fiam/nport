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
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};

use crate::cert;

use super::handlers;
use super::state::{AppState, SharedState};
use super::{
    config::{Hostnames, Listen},
    stats::Stats,
};

static DEFAULT_HTTPS_PORT: u16 = 443;
static API_CONNECT_PATH: &str = "/v1/connect";

async fn to_tls_middleware<B>(
    State(state): State<SharedState>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    if state.has_tls() && !state.via_tls() {
        let mut parts = request.uri().clone().into_parts();
        parts.scheme = Some(Scheme::HTTPS);
        let main_hostname = state.hostnames().main_hostname();
        let https_port = state.https_port();
        let authority_str = if https_port == DEFAULT_HTTPS_PORT {
            main_hostname.to_string()
        } else {
            format!("{}:{}", main_hostname, https_port)
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

async fn request_counter<B>(
    State(state): State<SharedState>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    state.stats().http_request();
    next.run(request).await
}

#[derive(Clone, Debug)]
pub struct Options {
    client_request_timeout_secs: u16,
    send_welcome_message: bool,
}

impl Options {
    pub fn new(client_request_timeout_secs: u16, send_welcome_message: bool) -> Self {
        Self {
            client_request_timeout_secs,
            send_welcome_message,
        }
    }

    pub fn client_request_timeout_secs(&self) -> u16 {
        self.client_request_timeout_secs
    }
    pub fn send_welcome_message(&self) -> bool {
        self.send_welcome_message
    }
}

pub struct Server {
    listen: Listen,
    hostnames: Hostnames,
    cert_store: Option<Arc<cert::Store>>,
    http_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    https_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    stats: Stats,
    options: Options,
}

impl Server {
    pub fn new(
        listen: Listen,
        hostnames: Hostnames,
        cert_store: Option<Arc<cert::Store>>,
        stats: Stats,
        options: Options,
    ) -> Self {
        Self {
            listen,
            hostnames,
            cert_store,
            http_shutdown: Mutex::new(None),
            https_shutdown: Mutex::new(None),
            stats,
            options,
        }
    }

    fn build_app(&self, state: SharedState) -> Router {
        let main_router = Router::new()
            .route("/", get(handlers::home))
            .route("/stats", get(handlers::stats))
            .route("/build_info.json", get(handlers::build_info))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                to_tls_middleware,
            ))
            .fallback_service(ServeDir::new("static"))
            .with_state(state.clone());

        let api_router = Router::new()
            .route(API_CONNECT_PATH, get(handlers::websocket))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                to_tls_middleware,
            ))
            .with_state(state.clone());

        let forwarding_router = Router::new()
            .route("/", any(handlers::forward))
            .route("/*path", any(handlers::forward))
            .with_state(state.clone());

        let main_hostname = self.hostnames.main_hostname().to_string();
        let api_hostname = self.hostnames.api_hostname().to_string();

        let chooser = |Host(hostname): Host, request: Request<Body>| async move {
            // Split port
            let host = if let Some(parts) = hostname.rsplit_once(':') {
                parts.0
            } else {
                &hostname
            };
            tracing::debug!(host, main_hostname, api_hostname, "routing request");
            let router = if host == api_hostname && request.uri().path() == API_CONNECT_PATH {
                tracing::trace!(host, "routing request to api");
                api_router
            } else if host == main_hostname {
                tracing::trace!(host, "routing request to main");
                main_router
            } else {
                tracing::trace!(host, "routing request to forwarding");
                forwarding_router
            };
            router.oneshot(request).await
        };

        Router::new()
            .route("/*path", any(chooser.clone()))
            .route("/", any(chooser))
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                request_counter,
            ))
            .layer(
                TraceLayer::new_for_http().make_span_with(
                    DefaultMakeSpan::new()
                        .include_headers(true)
                        .level(tracing::Level::INFO),
                ),
            )
    }

    async fn run_http(&self, state: SharedState) -> anyhow::Result<()> {
        if self.listen.http() == 0 {
            return Ok(());
        }
        let app = self.build_app(state);
        let addr = SocketAddr::from((self.listen.address(), self.listen.http()));
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
        if self.listen.https() == 0 {
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

        let addr = SocketAddr::from((self.listen.address(), self.listen.https()));
        tracing::debug!("listening for HTTPS on {}", addr);
        let server = axum_server::bind_rustls(addr, RustlsConfig::from_config(config.into()))
            .serve(app.into_make_service_with_connect_info::<SocketAddr>());
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
        let state = Arc::new(
            AppState::new(
                &self.listen,
                &self.hostnames,
                self.stats.clone(),
                &self.options,
            )
            .unwrap(),
        );

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
