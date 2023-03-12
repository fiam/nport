use std::{collections::HashMap, net::SocketAddr, ops::ControlFlow, sync::Arc, time::Duration};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{ws::WebSocket, ConnectInfo, OriginalUri, State, WebSocketUpgrade},
    headers::HeaderName,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use tokio::{sync::oneshot::Receiver, time::timeout};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use liblocalport as lib;

use crate::server::client::{self, Client};
use crate::server::state::SharedState;

const CLIENT_REQUEST_TIMEOUT_SECS: u64 = 30;

pub async fn websocket(
    State(state): State<SharedState>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket, addr))
}

async fn handle_socket(state: SharedState, socket: WebSocket, who: SocketAddr) {
    let client = state.registry().register(socket, who).await;
    loop {
        let request = client.recv().await;
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

    state.registry().deregister(client).await;
    debug!(who = ?who, "client disconnected");
}

async fn handle_message(
    state: &SharedState,
    client: Arc<Client>,
    msg: lib::client::Message,
) -> Result<ControlFlow<()>> {
    use lib::client::Message;

    match msg {
        Message::HttpOpen(open) => {
            return handle_message_http_open(state, client.clone(), open)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        Message::HttpClose(close) => {
            return handle_message_http_close(state, client.clone(), close)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        Message::HttpResponse(response) => {
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

async fn handle_message_http_open(
    state: &SharedState,
    client: Arc<Client>,
    open: lib::client::HttpOpen,
) -> Result<()> {
    use lib::server;

    let hostname = if !open.hostname.is_empty() {
        open.hostname
    } else {
        names::Generator::with_naming(names::Name::Numbered)
            .next()
            .unwrap()
    };
    if !state.registry().claim_http_hostname(&hostname).await {
        return client
            .send(&server::Message::HttpOpen(server::HttpOpen::failed(
                server::HttpOpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!(hostname, "HTTP forwarding opened");
    client.add_http_hostname(&hostname).await;
    return client
        .send(&server::Message::HttpOpen(server::HttpOpen::ok(
            &hostname,
            open.local_port,
        )))
        .await;
}

async fn handle_message_http_close(
    state: &SharedState,
    client: Arc<Client>,
    close: lib::client::HttpClose,
) -> Result<()> {
    let hostname = &close.hostname;
    let result = if !client.remove_http_hostname(hostname).await
        || !state.registry().release_http_hostname(hostname).await
    {
        lib::server::HttpCloseResult::NotRegistered
    } else {
        tracing::debug!(hostname, "HTTP forwarding closed");
        lib::server::HttpCloseResult::Ok
    };
    let response = lib::server::Message::HttpClose(lib::server::HttpClose {
        hostname: hostname.to_owned(),
        result,
    });
    return client.send(&response).await;
}

async fn handle_message_http_response(
    _: &SharedState,
    client: Arc<Client>,
    response: lib::client::HttpResponse,
) -> Result<()> {
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
            let response = match timeout(Duration::from_secs(CLIENT_REQUEST_TIMEOUT_SECS), rx).await
            {
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
            match response.payload {
                lib::client::HttpResponsePayload::Error(error) => {
                    info!(error = ?error, "error response");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
                lib::client::HttpResponsePayload::Data(data) => {
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
                        StatusCode::from_u16(data.status_code).unwrap(),
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
) -> Result<Receiver<lib::client::HttpResponse>> {
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
            let request = lib::server::HttpRequest {
                uuid,
                hostname: hostname.to_string(),
                addr: addr.to_string(),
                protocol: if state.via_tls() { "https" } else { "http" }.to_string(),
                method: method.to_string(),
                uri: original_uri,
                headers: header_map,
                body: body.to_vec(),
            };

            handler
                .send(&lib::server::Message::HttpRequest(request))
                .await?;

            Ok(rx)
        }
        None => Err(client::Error::ClientNotFound.into()),
    }
}

fn should_skip_header(header: &str) -> bool {
    header == "transfer-encoding"
}
