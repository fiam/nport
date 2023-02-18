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
use futures_util::stream::FuturesUnordered;
use futures_util::{SinkExt, StreamExt};
use std::ops::ControlFlow;
use std::time::Instant;
use tokio::task::JoinHandle;

use liblocalport as lib;

use client::Client;

use crate::error::{Error, Result};
use crate::transport::Transport;

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
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();

    let mut clients = FuturesUnordered::new();

    match args.command {
        Command::Http { port } => {
            clients.push(tokio::spawn(spawn_http_client(args.hostname, port)));
        }
    }

    //wait for all our clients to exit
    while clients.next().await.is_some() {}
}

async fn spawn_http_client(hostname: String, port: u16) {
    let client = Client::new(port);
    match client.connect().await {
        Ok(()) => {
            println!("connected");
            client
                .send(&lib::client::Message::Open(lib::client::Open {
                    hostname: hostname.to_owned(),
                }))
                .await;
        }
        Err(error) => {
            println!("can't connect {}", error);
            return;
        }
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
        server::Message::Open(open) => {
            println!("opened {}", open.hostname);
        }
        server::Message::HTTPRequest(req) => {
            println!("got request {:?}", req);
            dispatch_message_http_request(client, &req).await?
        }
    }

    Ok(())
}

async fn dispatch_message_http_request(
    client: &Client,
    req: &lib::server::HTTPRequest,
) -> Result<()> {
    use hyper::body::HttpBody;
    use hyper::{
        http::{Request, Response},
        Method,
    };

    let uri = format!("http://localhost:{}{}", client.port(), req.uri);
    let method = Method::from_bytes(req.method.as_bytes()).unwrap();
    let body = hyper::Body::from(req.body.clone());
    let request = Request::builder().uri(uri).method(method).body(body)?;
    dbg!(&request);
    let http_client = hyper::client::Client::new();
    let http_response = http_client.request(request).await?;
    let resp_status_code = http_response.status().as_u16();
    let resp_headers = http_response
        .headers()
        .into_iter()
        .map(|(key, value)| (key.as_str().to_owned(), value.as_bytes().to_owned()))
        .collect();
    let resp_body = hyper::body::to_bytes(http_response.into_body()).await?;
    let response = lib::client::HttpResponse {
        uuid: req.uuid.clone(),
        headers: resp_headers,
        body: resp_body.to_vec(),
        status_code: resp_status_code,
    };
    client
        .send(&lib::client::Message::HttpResponse(response))
        .await?;
    Ok(())
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
