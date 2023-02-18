use std::{collections::HashMap, net::SocketAddr, ops::ControlFlow, sync::Arc};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{ws::WebSocket, ConnectInfo, OriginalUri, State, WebSocketUpgrade},
    headers::HeaderName,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use tokio::sync::{oneshot::Receiver, RwLock};
use tracing::{debug, error, warn};
use uuid::Uuid;

use liblocalport as lib;

use super::client::Client;
use super::state::SharedState;

pub async fn websocket(
    State(state): State<SharedState>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket, addr))
}

async fn handle_socket(state: SharedState, socket: WebSocket, who: SocketAddr) {
    let client = Arc::new(RwLock::new(Client::new(socket, who)));
    loop {
        let request = client.read().await.recv().await;
        let result = match request {
            Ok(message) => handle_message(&state, client.clone(), message).await,
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

    state.lock().await.registry().deregister(client).await;
    debug!(who = ?who, "client disconnected");
}

async fn handle_message(
    state: &SharedState,
    client: Arc<RwLock<Client>>,
    msg: lib::client::Message,
) -> Result<ControlFlow<()>> {
    match msg {
        lib::client::Message::Open(open) => {
            return handle_message_open(state, client.clone(), open)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        lib::client::Message::HttpResponse(response) => {
            return handle_message_http_response(state, client.clone(), response)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        _ => {
            error!(message = ?msg, "handled message type");
        }
    }
    Ok(ControlFlow::Continue(()))
}

async fn handle_message_open(
    state: &SharedState,
    shared_client: Arc<RwLock<Client>>,
    open: lib::client::Open,
) -> Result<()> {
    use lib::server;

    let mut client = shared_client.write().await;
    if client.is_open() {
        return client
            .send(&server::Message::Open(server::Open::failed(
                server::OpenResult::AlreadyOpen,
            )))
            .await;
    }
    let hostname = if !open.hostname.is_empty() {
        open.hostname
    } else {
        names::Generator::with_naming(names::Name::Numbered)
            .next()
            .unwrap()
    };
    let state = state.lock().await;
    if !state
        .registry()
        .register(&hostname, shared_client.clone())
        .await
    {
        return client
            .send(&server::Message::Open(server::Open::failed(
                server::OpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!(hostname, "forwarding opened");
    client.set_hostname(&hostname);
    return client
        .send(&server::Message::Open(server::Open::ok(&hostname)))
        .await;
}

async fn handle_message_http_response(
    _: &SharedState,
    client: Arc<RwLock<Client>>,
    response: lib::client::HttpResponse,
) -> Result<()> {
    let client = client.read().await;
    client.send_response(response).await?;
    Ok(())
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
        state,
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
            let response = rx.await.unwrap();
            debug!(response.uuid, "HTTP response");
            let header_map = response
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
                StatusCode::from_u16(response.status_code).unwrap(),
                header_map,
                response.body,
            )
                .into_response()
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
) -> Result<Receiver<lib::client::HttpResponse>> {
    let handler = state.lock().await.registry().get(hostname).await;
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
            let rx = handler.read().await.register(&uuid).await;
            let request = lib::server::HTTPRequest {
                uuid,
                addr: addr.to_string(),
                protocol: "proto".to_string(),
                method: method.to_string(),
                uri: original_uri,
                headers: header_map,
                body: body.to_vec(),
            };

            handler
                .read()
                .await
                .send(&lib::server::Message::HTTPRequest(request))
                .await?;

            Ok(rx)
        }
        None => Err(super::client::Error::ClientNotFound.into()),
    }
}

fn should_skip_header(header: &str) -> bool {
    header == "transfer-encoding"
}
