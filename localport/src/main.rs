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

use clap::Parser;
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use liblocalport as lib;

use client::Client;

use crate::error::Result;

const SERVER: &'static str = "ws://127.0.0.1:3000/v1/connect";

#[derive(clap::Parser)]
struct Arguments {
    #[arg(long, short = 'H')]
    hostname: String,
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

    let client = Client::new();

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
        Command::Tcp { port } => client.tcp_open(&args.hostname, 0, port).await,
    };

    if let Err(error) = result {
        tracing::error!(error=?error, "can't open connection");
        return;
    }

    loop {
        match client.recv().await {
            Ok(msg) => {
                println!("got msg {:?}", msg);
                if let Err(error) = dispatch_message(&client, msg).await {
                    println!("error handling msg {:?}", error);
                }
            }
            Err(error) => {
                println!("recv error {:?}", error);
                return;
            }
        }
    }
}

async fn dispatch_message(client: &Client, msg: lib::server::Message) -> Result<()> {
    use lib::server;

    match msg {
        server::Message::HttpOpen(open) => {
            if let Err(err) = client.http_register(&open).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(
                    hostname = open.hostname,
                    local_port = open.local_port,
                    "HTTP host created"
                )
            }
        }
        server::Message::HttpRequest(req) => {
            println!("got request {:?}", req);
            dispatch_message_http_request(client, &req).await?
        }
        server::Message::HttpClose(close) => {
            if let Err(err) = client.http_deregister(&close).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(hostname = close.hostname, "HTTP host closed")
            }
        }
    }

    Ok(())
}

async fn dispatch_message_http_request(
    client: &Client,
    req: &lib::server::HttpRequest,
) -> Result<()> {
    let payload = match send_http_request(client, req).await {
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
    client: &Client,
    req: &lib::server::HttpRequest,
) -> std::result::Result<lib::client::HttpResponseData, lib::client::HttpResponseError> {
    use hyper::{http::Request, Method};
    use lib::client::HttpResponseError;

    match client.http_port(&req.hostname).await {
        None => Err(HttpResponseError::NotRegistered),
        Some(port) => {
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
    }
}

// Function to handle messages we get (with a slight twist that Frame variant is visible
// since we are working with the underlying tungstenite library directly without axum here).
// fn process_message(msg: Message, who: usize) -> ControlFlow<(), ()> {
//     match msg {
//         Message::Text(t) => {
//             println!(">>> {} got str: {:?}", who, t);
//         }
//         Message::Binary(d) => {
//             println!(">>> {} got {} bytes: {:?}", who, d.len(), d);
//         }
//         Message::Close(c) => {
//             if let Some(cf) = c {
//                 println!(
//                     ">>> {} got close with code {} and reason `{}`",
//                     who, cf.code, cf.reason
//                 );
//             } else {
//                 println!(">>> {} somehow got close message without CloseFrame", who);
//             }
//             return ControlFlow::Break(());
//         }

//         Message::Pong(v) => {
//             println!(">>> {} got pong with {:?}", who, v);
//         }
//         // Just as with axum server, the underlying tungstenite websocket library
//         // will handle Ping for you automagically by replying with Pong and copying the
//         // v according to spec. But if you need the contents of the pings you can see them here.
//         Message::Ping(v) => {
//             println!(">>> {} got ping with {:?}", who, v);
//         }

//         Message::Frame(_) => {
//             unreachable!("This is never supposed to happen")
//         }
//     }
//     ControlFlow::Continue(())
// }
