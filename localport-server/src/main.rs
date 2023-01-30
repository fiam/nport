//! Example websocket server.
//!
//! Run the server with
//! ```not_rust
//! cargo run -p example-websockets --bin example-websockets
//! ```
//!
//! Run a browser client with
//! ```not_rust
//! firefox http://localhost:3000
//! ```
//!
//! Alternatively you can run the rust client (showing two
//! concurrent websocket connections being established) with
//! ```not_rust
//! cargo run -p example-websockets --bin example-client
//! ```

use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Host, State, TypedHeader,
    },
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, get_service},
    Router,
};
use liblocalport::{client, server};
use names;
use registry::Registry;
use tokio::{runtime::Handle, sync::Mutex};

use std::{borrow::Cow, sync::Arc};
use std::{net::SocketAddr, path::PathBuf};
use std::{ops::ControlFlow, sync::RwLock};
use tower::ServiceExt;
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

//allows to extract the IP of connecting user
use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::CloseFrame;

//allows to split the websocket stream into separate TX and RX branches
use futures::{future::Shared, sink::SinkExt, stream::StreamExt};

mod handler;
mod registry;

#[derive(Default)]
struct AppState {
    registry: Registry,
}

impl AppState {
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

type SharedState = Arc<Mutex<AppState>>;

#[tokio::main]
async fn main() {
    let state = SharedState::default();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_websockets=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let host_router = Router::new()
        .route("/v1/connect", get(ws_handler))
        .with_state(state.clone());

    let forwarding_router = Router::new()
        .route("/*path", any(forwarding_handler))
        .with_state(state.clone());

    let app = Router::new()
        .route(
            "/*path",
            any(|Host(hostname): Host, request: Request<Body>| async move {
                println!("hostname received is {}", hostname);
                match hostname.as_str() {
                    "127.0.0.1:3000" => host_router.oneshot(request).await,
                    _ => forwarding_router.oneshot(request).await,
                }
            }),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state);

    // run it with hyper
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

/// The handler for the HTTP request (this gets called when the HTTP GET lands at the start
/// of websocket negotiation). After this completes, the actual switching from HTTP to
/// websocket protocol will occur.
/// This is the last point where we can extract TCP/IP metadata such as IP address of the client
/// as well as things from HTTP headers such as user-agent of the browser etc.
async fn ws_handler(
    State(state): State<SharedState>,
    ws: WebSocketUpgrade,
    host: Option<TypedHeader<axum::headers::Host>>,
    user_agent: Option<TypedHeader<axum::headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    if let Some(TypedHeader(host)) = host {
        println!("host header {}", host);
    }
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    println!("`{}` at {} connected.", user_agent, addr.to_string());
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| handle_socket(state, socket, addr))
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(state: SharedState, socket: WebSocket, who: SocketAddr) {
    let handler = Arc::new(Mutex::new(handler::Handler::new(socket, who)));
    //send a ping (unsupported by some browsers) just to kick things off and get a response
    // if let Ok(_) = socket.send(Message::Ping(vec![1, 2, 3])).await {
    //     println!("Pinged {}...", who);
    // } else {
    //     println!("Could not send ping {}!", who);
    //     // no Error here since the only thing we can do is to close the connection.
    //     // If we can not send messages, there is no way to salvage the statemachine anyway.
    //     return;
    // }
    loop {
        let request = handler.lock().await.recvRequest().await;
        let result = match request {
            Ok(request) => handle_request(&state, handler.clone(), request).await,
            Err(error) => {
                println!("client failed {}", error);
                Err(error)
            }
        };
        match result {
            Ok(flow) => match flow {
                ControlFlow::Continue(_) => {}
                _ => {
                    println!("client done");
                    break;
                }
            },
            Err(error) => {
                println!("client failed {}", error);
                break;
            }
        }
    }

    // match handler.start().await {
    //     Ok(hostname) => {
    //         let hostname = if hostname.len() > 0 {
    //             hostname
    //         } else {
    //             names::Generator::with_naming(names::Name::Numbered)
    //                 .next()
    //                 .unwrap()
    //         };
    //         let state = state.lock().await;
    //         if !state.registry().register(&hostname, handler).await {
    //             println!("failed to registered {}", hostname);
    //             return;
    //         }
    //         println!("registered {}", hostname);
    //     }
    //     Err(error) => {
    //         println!("error {}", error);
    //     }
    // }

    // receive single message from a client (we can either receive or send with socket).
    // this will likely be the Pong for our Ping or a hello message from client.
    // waiting for message from a client will block this task, but will not block other client's
    // connections.

    // Since each client gets individual statemachine, we can pause handling
    // when necessary to wait for some external event (in this case illustrated by sleeping).
    // Waiting for this client to finish getting its greetings does not prevent other clients from
    // connecting to server and receiving their greetings.
    // for i in 1..5 {
    //     if socket
    //         .send(Message::Text(String::from(format!("Hi {} times!", i))))
    //         .await
    //         .is_err()
    //     {
    //         println!("client {} abruptly disconnected", who);
    //         return;
    //     }
    //     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    // }

    // By splitting socket we can send and receive at the same time. In this example we will send
    // unsolicited messages to client based on some sort of server's internal event (i.e .timer).
    //    let (mut sender, mut receiver) = socket.split();

    // Spawn a task that will push several messages to the client (does not matter what client does)
    // let mut send_task = tokio::spawn(async move {
    //     let n_msg = 20;
    //     for i in 0..n_msg {
    //         // In case of any websocket error, we exit.
    //         if sender
    //             .send(Message::Text(format!("Server message {} ...", i)))
    //             .await
    //             .is_err()
    //         {
    //             return i;
    //         }

    //         tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    //     }

    //     println!("Sending close to {}...", who);
    //     if let Err(e) = sender
    //         .send(Message::Close(Some(CloseFrame {
    //             code: axum::extract::ws::close_code::NORMAL,
    //             reason: Cow::from("Goodbye"),
    //         })))
    //         .await
    //     {
    //         println!("Could not send Close due to {}, probably it is ok?", e);
    //     }
    //     n_msg
    // });

    // This second task will receive messages from client and print them on server console
    // let mut recv_task = tokio::spawn(async move {
    //     let mut cnt = 0;
    //     while let Some(Ok(msg)) = receiver.next().await {
    //         cnt += 1;
    //         // print message and break if instructed to do so
    //         if process_message(msg, who).is_break() {
    //             break;
    //         }
    //     }
    //     cnt
    // });

    // If any one of the tasks exit, abort the other.
    // tokio::select! {
    //     rv_a = (&mut send_task) => {
    //         match rv_a {
    //             Ok(a) => println!("{} messages sent to {}", a, who),
    //             Err(a) => println!("Error sending messages {:?}", a)
    //         }
    //         recv_task.abort();
    //     },
    //     rv_b = (&mut recv_task) => {
    //         match rv_b {
    //             Ok(b) => println!("Received {} messages", b),
    //             Err(b) => println!("Error receiving messages {:?}", b)
    //         }
    //         send_task.abort();
    //     }
    // }

    // returning from the handler closes the websocket connection
    println!("Websocket context {} destroyed", who);
}

async fn handle_request(
    state: &SharedState,
    shared_handler: Arc<Mutex<handler::Handler>>,
    request: client::request::Request,
) -> Result<ControlFlow<()>> {
    match request {
        client::request::Request::Open(open) => {
            return handle_request_open(&state, shared_handler.clone(), open)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
    }
    Ok(ControlFlow::Continue(()))
}

async fn handle_request_open(
    state: &SharedState,
    shared_handler: Arc<Mutex<handler::Handler>>,
    open: client::request::Open,
) -> Result<()> {
    use liblocalport::server::response;
    let mut handler = shared_handler.lock().await;
    if handler.is_open() {
        return handler
            .send(&response::Response::Open(response::Open::failed(
                response::OpenResult::AlreadyOpen,
            )))
            .await;
    }
    let hostname = if open.hostname.len() > 0 {
        open.hostname
    } else {
        names::Generator::with_naming(names::Name::Numbered)
            .next()
            .unwrap()
    };
    let state = state.lock().await;
    if !state
        .registry()
        .register(&hostname, shared_handler.clone())
        .await
    {
        return handler
            .send(&response::Response::Open(response::Open::failed(
                response::OpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!("client registered on {}", hostname);
    handler.set_hostname(&hostname);
    return handler
        .send(&response::Response::Open(response::Open::ok(&hostname)))
        .await;
}

async fn forwarding_handler(
    State(state): State<SharedState>,
    host: Option<TypedHeader<axum::headers::Host>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    let hostname = match host {
        Some(header) => header.to_string(),
        None => "".to_owned(),
    };

    match state.lock().await.registry().get(&hostname).await {
        Some(handler) => {
            return (StatusCode::SERVICE_UNAVAILABLE, "Unimplemented").into_response();
        }
        None => {
            return (StatusCode::NOT_FOUND, "Not Found").into_response();
        }
    }
}
