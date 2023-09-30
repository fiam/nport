use std::{collections::HashMap, net::SocketAddr, ops::ControlFlow, time::Duration};

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
        server::{payload::Message, HttpScheme},
    },
    Addr,
};
use tokio::{sync::oneshot::Receiver, time::timeout};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::server::client;
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

async fn handle_socket(state: SharedState, socket: WebSocket, who: SocketAddr) {
    let client = state.registry().register(socket, who).await;
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

    match rx {
        Ok(rx) => {
            let response_timeout = Duration::from_secs(state.client_request_timeout_secs().into());
            let response = match timeout(response_timeout, rx).await {
                Ok(result) => match result {
                    Ok(response) => response,
                    Err(error) => {
                        warn!(error=?error, "could not receive HTTP response");
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                },
                Err(error) => {
                    warn!(error=?error, "timeout awaiting HTTP response");
                    return StatusCode::GATEWAY_TIMEOUT.into_response();
                }
            };
            debug!(response.uuid, "HTTP response");
            let Some(payload) = response.payload else {
                warn!("empty response payload");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            match payload {
                messages::client::http_response::Payload::Error(error) => {
                    info!(error = ?error, "error response");
                    StatusCode::BAD_GATEWAY.into_response()
                }
                messages::client::http_response::Payload::Data(data) => {
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
                                        warn!(
                                            "skipping invalid header value {:?}: {}",
                                            value, error
                                        );
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
            }
        }
        Err(error) => {
            warn!("enqueueing {}", error);
            (StatusCode::NOT_FOUND, "Not Found").into_response()
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
    match handler {
        Some(handler) => {
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
        None => {
            tracing::debug!(host = hostname, "no forwarding found");
            Err(client::Error::ClientNotFound.into())
        }
    }
}

fn should_skip_header(header: &str) -> bool {
    header == "transfer-encoding"
}
