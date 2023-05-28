mod client;
mod error;
mod settings;
mod transport;

use std::sync::Arc;

use clap::Parser;
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use client::Client;

use crate::error::Result;

#[derive(clap::Parser)]
struct Arguments {
    #[arg(long, short = 'H')]
    hostname: Option<String>,
    #[arg(long, short = 'R')]
    remote_port: Option<u16>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    Http { local_port: u16 },
    Tcp { local_port: u16 },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Arguments::parse();

    let s = match settings::Settings::new() {
        Ok(settings) => settings,
        Err(error) => {
            tracing::error!(error=?error, "parsing configuration");
            return;
        }
    };

    let mut tunnels = s.tunnels.unwrap_or(Vec::new());
    if let Some(command) = args.command {
        match command {
            Command::Http { local_port } => {
                tunnels.push(settings::Tunnel::Http(settings::HttpTunnel {
                    hostname: args.hostname,
                    local_port,
                }))
            }
            Command::Tcp { local_port } => {
                tunnels.push(settings::Tunnel::Tcp(settings::TcpTunnel {
                    hostname: args.hostname,
                    local_port,
                    remote_port: args.remote_port,
                }))
            }
        }
    }

    if tunnels.is_empty() {
        tracing::error!("no tunnels to run");
        return;
    }

    let client = Arc::new(Client::new());

    match client.connect(&s.server.hostname).await {
        Ok(()) => {
            tracing::info!(server = s.server.hostname, "connected");
        }
        Err(error) => {
            tracing::error!(error=?error, server=s.server.hostname, "can't connect to server");
            return;
        }
    }

    for tunnel in tunnels {
        let result = match tunnel {
            settings::Tunnel::Http(http) => {
                client
                    .http_open(&http.hostname.unwrap_or_default(), http.local_port)
                    .await
            }
            settings::Tunnel::Tcp(tcp) => {
                client
                    .tcp_open(
                        &tcp.hostname.unwrap_or_default(),
                        tcp.remote_port.unwrap_or_default(),
                        tcp.local_port,
                    )
                    .await
            }
        };
        if let Err(error) = result {
            tracing::error!(error=?error, "can't open connection");
            return;
        }
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

async fn dispatch_message(client: Arc<Client>, msg: libnp::server::Message) -> Result<()> {
    use libnp::server;

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
    req: &libnp::server::HttpRequest,
) -> Result<()> {
    let payload = match send_http_request(client.clone(), req).await {
        Ok(data) => libnp::client::HttpResponsePayload::Data(data),
        Err(error) => libnp::client::HttpResponsePayload::Error(error),
    };
    debug!(payload = ?payload, "HTTP response payload");
    let response = libnp::client::HttpResponse {
        uuid: req.uuid.clone(),
        payload,
    };
    client
        .send(&libnp::client::Message::HttpResponse(response))
        .await?;
    Ok(())
}

async fn send_http_request(
    client: Arc<Client>,
    req: &libnp::server::HttpRequest,
) -> std::result::Result<libnp::client::HttpResponseData, libnp::client::HttpResponseError> {
    use hyper::{http::Request, Method};
    use libnp::client::HttpResponseError;

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
    Ok(libnp::client::HttpResponseData {
        headers: resp_headers,
        body: resp_body.to_vec(),
        status_code: resp_status_code,
    })
}

async fn dispatch_message_port_connect(
    client: Arc<Client>,
    connect: &libnp::server::PortConnect,
) -> Result<()> {
    use libnp::client::{Message, PortConnected, PortConnectedResult};

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
