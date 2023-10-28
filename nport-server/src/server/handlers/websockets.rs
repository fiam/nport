use std::{collections::HashMap, fmt::Display, net::SocketAddr, ops::ControlFlow, time::Duration};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{ws::WebSocket, ConnectInfo, OriginalUri, Query, State, WebSocketUpgrade},
    headers::HeaderName,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};

use libnp::{
    messages::{
        self,
        server::{payload::Message, HttpScheme, PrintLevel},
    },
    Addr,
};
use tokio::{sync::oneshot::Receiver, time::timeout};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::server::{build_info::BuildInfo, client};
use crate::server::{msghandlers, state::SharedState};

pub async fn websocket(
    State(state): State<SharedState>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    tracing::debug!(params = ?params, "new websocket connection");
    ws.on_upgrade(move |socket| handle_socket(state, socket, addr))
}

/// Sends a welcome message to a client that has just connected, unless the server config
/// has disabled client welcome messages, in that case it returns Ok().
async fn send_welcome(state: &SharedState, client: &client::Client) -> Result<()> {
    if !state.options().send_welcome_message() {
        return Ok(());
    }
    let build_info = BuildInfo::default();
    let welcome = messages::server::payload::Message::Print(messages::server::Print {
        message: format!(
            "Connected to nport server on {} - v{} ({}) built on {}",
            state.hostnames().domain(),
            build_info.pkg_version,
            build_info.short_commit_hash,
            build_info.build_time,
        ),
        level: PrintLevel::Info as i32,
    });
    client.send(welcome).await
}

async fn handle_socket(state: SharedState, socket: WebSocket, who: SocketAddr) {
    let client = state.registry().register(socket, who).await;
    if let Err(error) = send_welcome(&state, &client).await {
        tracing::warn!(error=?error, "could not send welcome message, disconnecting");
        return;
    }
    loop {
        let request = client.recv().await;
        let result = match request {
            Ok(message) => msghandlers::msg(&state, client.clone(), message).await,
            Err(error) => Err(error),
        };
        match result {
            Ok(flow) => match flow {
                ControlFlow::Continue(_) => {}
                _ => {
                    debug!(who = ?who, "client done");
                    break;
                }
            },
            Err(error) => {
                debug!(who = ?who, error = ?error, "client failed");
                break;
            }
        }
    }

    state.registry().deregister(client).await;
    debug!(who = ?who, "client disconnected");
}

#[derive(Debug)]
struct HttpResponseErrorDisplayer(pub messages::client::HttpResponseError);

impl From<messages::client::HttpResponseError> for HttpResponseErrorDisplayer {
    fn from(error: messages::client::HttpResponseError) -> Self {
        Self(error)
    }
}

impl Display for HttpResponseErrorDisplayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(ref error) = self.0.error else {
            return write!(f, "unknown error");
        };
        use messages::client::http_response_error::Error;
        match error {
            Error::ForwardingNotRegistered(error) => {
                write!(f, "forwarding not registered for {}", error.hostname)
            }
            Error::InvalidMethod(error) => {
                write!(f, "invalid HTTP method {}: {}", error.method, error.error)
            }
            Error::InvalidHeaderName(error) => {
                write!(f, "invalid header name for: {}", error.header_name,)
            }
            Error::InvalidHeaderValue(error) => {
                write!(
                    f,
                    "invalid header value for {}: {}",
                    error.header_name, error.header_value,
                )
            }
            Error::CreatingRequest(error) => {
                write!(f, "error creating request: {}", error.error)
            }
            Error::FetchingResponse(error) => {
                write!(f, "error fetching response: {}", error.error)
            }
            Error::ReadingResponse(error) => {
                write!(f, "error reading response: {}", error.error)
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
enum ForwardRequestError {
    #[error("no client registered for this domain")]
    ClientNotFound,
    #[error("empty response from client")]
    EmptyResponsePayload,
    #[error("timeout awaiting HTTP response")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("error receiving HTTP response: {0}")]
    Recv(#[from] tokio::sync::oneshot::error::RecvError), // 500

    #[error("error from client: {0}")]
    ErrorResponse(HttpResponseErrorDisplayer), // BAD_GATEWAY
}

impl ForwardRequestError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ForwardRequestError::ClientNotFound => StatusCode::NOT_FOUND,
            ForwardRequestError::EmptyResponsePayload => StatusCode::INTERNAL_SERVER_ERROR,
            ForwardRequestError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            ForwardRequestError::Recv(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ForwardRequestError::ErrorResponse(_) => StatusCode::BAD_GATEWAY,
        }
    }
}

async fn forward_request(
    state: &SharedState,
    rx: Result<Receiver<messages::client::HttpResponse>>,
) -> std::result::Result<messages::client::HttpResponseData, ForwardRequestError> {
    let Ok(rx) = rx else {
        return Err(ForwardRequestError::ClientNotFound);
    };
    let response_timeout =
        Duration::from_secs(state.options().client_request_timeout_secs().into());
    let result = timeout(response_timeout, rx).await?;
    let response = result?;
    tracing::debug!(response.uuid, "HTTP response");
    let Some(payload) = response.payload else {
        tracing::warn!("empty response payload");
        return Err(ForwardRequestError::EmptyResponsePayload);
    };
    match payload {
        messages::client::http_response::Payload::Error(error) => {
            tracing::info!(error = ?error, "error response");
            Err(ForwardRequestError::ErrorResponse(error.into()))
        }
        messages::client::http_response::Payload::Data(data) => Ok(data),
    }
}

pub async fn forward(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    method: Method,
    OriginalUri(original_uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let hostname = if let Some(host) = headers.get("host") {
        host.to_str().unwrap_or_default().to_string()
    } else {
        "".to_string()
    };

    let rx = enqueue_request(
        state.clone(),
        &hostname,
        addr,
        method,
        original_uri.to_string(),
        headers,
        body,
    )
    .await;

    match forward_request(&state, rx).await {
        Ok(data) => {
            let header_map = data
                .headers
                .into_iter()
                .filter_map(|(name, value)| {
                    if should_skip_header(&name) {
                        None
                    } else {
                        let name = match HeaderName::from_bytes(name.as_bytes()) {
                            Ok(name) => name,
                            Err(error) => {
                                warn!(name, "skipping invalid header name: {}", error);
                                return None;
                            }
                        };
                        let value = match HeaderValue::from_bytes(&value) {
                            Ok(value) => value,
                            Err(error) => {
                                warn!("skipping invalid header value {:?}: {}", value, error);
                                return None;
                            }
                        };
                        Some((name, value))
                    }
                })
                .collect::<HeaderMap>();
            (
                StatusCode::from_u16(data.status_code as u16).unwrap(),
                header_map,
                data.body,
            )
                .into_response()
        }
        Err(error) => {
            match &error {
                ForwardRequestError::ClientNotFound => {
                    tracing::warn!("enqueueing {}", error);
                }
                ForwardRequestError::EmptyResponsePayload => {
                    tracing::warn!("empty response payload");
                }
                ForwardRequestError::Timeout(error) => {
                    tracing::warn!(error=?error, "timeout awaiting HTTP response");
                }
                ForwardRequestError::Recv(error) => {
                    tracing::warn!(error=?error, "could not receive HTTP response");
                }
                ForwardRequestError::ErrorResponse(error) => {
                    tracing::info!(error = ?error, "error response");
                }
            };

            let mut renderer = state.templates().renderer("error.html");
            let status_code = error.status_code();
            let mut context = tera::Context::new();
            context.insert("hostname", &hostname);
            context.insert("error", &error.to_string());
            context.insert("http_error", &status_code.to_string());
            renderer
                .with_status(status_code)
                .with_context(context)
                .render_response()
                .into_response()
        }
    }
}

async fn enqueue_request(
    state: SharedState,
    hostname: &str,
    addr: SocketAddr,
    method: Method,
    original_uri: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Receiver<messages::client::HttpResponse>> {
    let handler = state.registry().get_by_http_hostname(hostname).await;
    let Some(handler) = handler else {
        tracing::debug!(host = hostname, "no forwarding found");
        return Err(client::Error::ClientNotFound.into());
    };
    let uuid = Uuid::new_v4().to_string();
    let mut previous_name: Option<HeaderName> = None;
    let header_map = headers
        .into_iter()
        .map(move |(k, v)| {
            let name = k.unwrap_or_else(|| previous_name.as_ref().unwrap().clone());
            previous_name = Some(name.clone());
            (name.as_str().to_string(), v.as_bytes().to_vec())
        })
        .collect::<HashMap<String, Vec<u8>>>();
    let rx = handler.register_http_request(&uuid).await;
    let request = messages::server::HttpRequest {
        uuid,
        hostname: hostname.to_string(),
        http_client_address: Some(Addr::from_socket_addr(&addr).to_address()),
        scheme: if state.via_tls() {
            HttpScheme::Http
        } else {
            HttpScheme::Https
        } as i32,
        method: method.to_string(),
        uri: original_uri,
        headers: header_map,
        body: Some(body.to_vec()),
    };

    handler.send(Message::HttpRequest(request)).await?;

    Ok(rx)
}

fn should_skip_header(header: &str) -> bool {
    header == "transfer-encoding"
}
