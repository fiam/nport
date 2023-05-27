//! Based on tokio-tungstenite example websocket client, but with multiple
//! concurrent websocket clients in one package
//!
//! This will connect to a server specified in the SERVER with N_CLIENTS
//! concurrent connections, and then flood some test messages over websocket.
//! This will also print whatever it gets into stdout.
//!
//! Note that this is not currently optimized for performance, especially around
//! stdout mutex management. Rather it's intended to show an example of working with axum's
//! websocket server and how the client-side and server-side code can be quite similar.
//!
//!
//!

mod client;
mod error;
mod transport;

use std::sync::Arc;

use clap::Parser;
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use liblocalport as lib;

use client::Client;

use crate::error::Result;

const SERVER: &str = "ws://127.0.0.1:3000/v1/connect";

#[derive(clap::Parser)]
struct Arguments {
    #[arg(long, short = 'H')]
    hostname: String,
    #[arg(long, short = 'R')]
    remote_port: u16,
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Http { port: u16 },
    Tcp { port: u16 },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Arguments::parse();

    let client = Arc::new(Client::new());

    match client.connect().await {
        Ok(()) => {
            tracing::info!(server = SERVER, "connected");
        }
        Err(error) => {
            tracing::error!(error=?error, "can't connect to server");
            return;
        }
    }

    let result = match args.command {
        Command::Http { port } => client.http_open(&args.hostname, port).await,
        Command::Tcp { port } => {
            client
                .tcp_open(&args.hostname, args.remote_port, port)
                .await
        }
    };

    if let Err(error) = result {
        tracing::error!(error=?error, "can't open connection");
        return;
    }

    loop {
        match client.recv().await {
            Ok(msg) => {
                tracing::trace!(msg=?msg, "server message");
                if let Err(error) = dispatch_message(client.clone(), msg).await {
                    tracing::error!(error=?error, "handling server message");
                }
            }
            Err(error) => {
                tracing::error!(error=?error, "error receiving data from server");
                return;
            }
        }
    }
}

async fn dispatch_message(client: Arc<Client>, msg: lib::server::Message) -> Result<()> {
    use lib::server;

    match msg {
        server::Message::HttpOpened(opened) => {
            if let Err(err) = client.http_register(&opened).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(
                    hostname = opened.hostname,
                    local_port = opened.local_port,
                    "HTTP host created"
                )
            }
        }
        server::Message::HttpRequest(req) => {
            tracing::debug!(request=?req, "incoming HTTP request");
            dispatch_message_http_request(client, &req).await?
        }
        server::Message::HttpClosed(closed) => {
            if let Err(err) = client.http_deregister(&closed).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(hostname = closed.hostname, "HTTP host closed")
            }
        }
        server::Message::PortOpened(opened) => {
            if let Err(err) = client.port_register(&opened).await {
                tracing::error!(error=?err,origin=opened.origin(), local_port=opened.local_port, "registering port forwarding");
            } else {
                tracing::info!(
                    origin = opened.origin(),
                    local_port = opened.local_port,
                    "port forwarding created"
                )
            }
        }
        server::Message::PortConnect(connect) => {
            tracing::debug!(origin = connect.origin(), "port connect");
            if let Err(err) = dispatch_message_port_connect(client, &connect).await {
                tracing::error!(error=?err,origin=connect.origin(), "connecting to port");
            }
        }
        server::Message::PortReceive(receive) => {
            tracing::debug!(
                uuid = receive.uuid,
                len = receive.data.len(),
                "received port data"
            );
            if let Err(err) = client.port_receive(&receive).await {
                tracing::error!(error=?err,uuid=receive.uuid, "writing to port");
            }
        }
        server::Message::PortClose(close) => {
            tracing::debug!(uuid = close.uuid, "requested to close port");
            if let Err(err) = client.port_close(&close).await {
                tracing::error!(error=?err,uuid=close.uuid, "closing port");
            }
        }
    }

    Ok(())
}

async fn dispatch_message_http_request(
    client: Arc<Client>,
    req: &lib::server::HttpRequest,
) -> Result<()> {
    let payload = match send_http_request(client.clone(), req).await {
        Ok(data) => lib::client::HttpResponsePayload::Data(data),
        Err(error) => lib::client::HttpResponsePayload::Error(error),
    };
    debug!(payload = ?payload, "HTTP response payload");
    let response = lib::client::HttpResponse {
        uuid: req.uuid.clone(),
        payload,
    };
    client
        .send(&lib::client::Message::HttpResponse(response))
        .await?;
    Ok(())
}

async fn send_http_request(
    client: Arc<Client>,
    req: &lib::server::HttpRequest,
) -> std::result::Result<lib::client::HttpResponseData, lib::client::HttpResponseError> {
    use hyper::{http::Request, Method};
    use lib::client::HttpResponseError;

    let port = match client.http_port(&req.hostname).await {
        None => return Err(HttpResponseError::NotRegistered),
        Some(port) => port,
    };
    let uri = format!("http://localhost:{}{}", port, req.uri);
    let method = Method::from_bytes(req.method.as_bytes())
        .map_err(|e| HttpResponseError::InvalidMethod(e.to_string()))?;
    let body = hyper::Body::from(req.body.clone());
    let request = Request::builder()
        .uri(uri)
        .method(method)
        .body(body)
        .map_err(|e| HttpResponseError::Build(e.to_string()))?;
    let http_client = hyper::client::Client::new();
    let http_response = http_client
        .request(request)
        .await
        .map_err(|e| HttpResponseError::Request(e.to_string()))?;
    let resp_status_code = http_response.status().as_u16();
    let resp_headers = http_response
        .headers()
        .into_iter()
        .map(|(key, value)| (key.as_str().to_owned(), value.as_bytes().to_owned()))
        .collect();
    let resp_body = hyper::body::to_bytes(http_response.into_body())
        .await
        .map_err(|e| HttpResponseError::Read(e.to_string()))?;
    Ok(lib::client::HttpResponseData {
        headers: resp_headers,
        body: resp_body.to_vec(),
        status_code: resp_status_code,
    })
}

async fn dispatch_message_port_connect(
    client: Arc<Client>,
    connect: &lib::server::PortConnect,
) -> Result<()> {
    use lib::client::{Message, PortConnected, PortConnectedResult};

    let client_copy = client.clone();
    let result = match client
        .port_connect(connect, |uuid, addr| async move {
            crate::client::port::start(client_copy, &uuid, &addr).await
        })
        .await
    {
        Ok(()) => PortConnectedResult::Ok,
        Err(error) => PortConnectedResult::Error(error.to_string()),
    };
    client
        .send(&Message::PortConnected(PortConnected {
            uuid: connect.uuid.clone(),
            result,
        }))
        .await
}
