use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::Host,
    http::Request,
    routing::{any, get},
    Router,
};
use tower::ServiceExt;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use state::SharedState;

mod client;
mod handlers;
mod registry;
mod state;

pub struct Server {
    port: u16,
    public_hostname: String,
}

impl Server {
    pub fn new<T: AsRef<str>>(port: u16, public_hostname: T) -> Self {
        return Server {
            port,
            public_hostname: public_hostname.as_ref().to_string(),
        };
    }

    fn public_hostname(&self) -> String {
        if !self.public_hostname.is_empty() {
            self.public_hostname.clone()
        } else {
            format!("127.0.0.1:{}", self.port)
        }
    }

    pub async fn run(&self) {
        let state = SharedState::default();
        let host_router = Router::new()
            .route("/v1/connect", get(handlers::websocket))
            .with_state(state.clone());

        let forwarding_router = Router::new()
            .route("/*path", any(handlers::forward))
            .with_state(state.clone());

        let public_hostname = self.public_hostname();
        let app = Router::new()
            .route(
                "/*path",
                any(|Host(hostname): Host, request: Request<Body>| async move {
                    tracing::trace!(hostname, "routing request");
                    let router = if hostname == public_hostname {
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
            .with_state(state);

        // run it with hyper
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::debug!("listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();
    }
}
