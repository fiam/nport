use std::sync::Arc;
use std::{net::SocketAddr, ops::ControlFlow};

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::stream::SplitStream;
use futures::SinkExt;
use futures::StreamExt;
use thiserror;

use liblocalport as lib;
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("client disconnected")]
    Disconnected,
    #[error("unexpected client message type")]
    UnexpectedMessageType,
    #[error("unexpected client message")]
    UnexpectedMessage,
}

pub struct Handler {
    hostname: String,
    ws_sender: Mutex<SplitSink<WebSocket, Message>>,
    ws_receiver: Mutex<SplitStream<WebSocket>>,
    who: SocketAddr,
}

impl Handler {
    pub fn new(socket: WebSocket, who: SocketAddr) -> Handler {
        let (ws_sender, ws_receiver) = socket.split();
        Handler {
            hostname: "".to_owned(),
            ws_sender: Mutex::new(ws_sender),
            ws_receiver: Mutex::new(ws_receiver),
            who,
        }
    }

    pub fn is_open(&self) -> bool {
        self.hostname.len() > 0
    }

    pub fn set_hostname(&mut self, hostname: &str) {
        self.hostname = hostname.to_owned();
    }

    async fn recvData(&self) -> Result<Vec<u8>> {
        println!("begin receive");
        let mut receiver = self.ws_receiver.lock().await;
        let r = match receiver.next().await {
            Some(received) => match received? {
                Message::Binary(data) => Ok(data),
                _ => Err(Error::UnexpectedMessageType.into()),
            },
            None => Err(Error::Disconnected.into()),
        };
        println!("end receive");
        return r;
    }

    pub async fn recvRequest(&self) -> Result<lib::client::Message> {
        let data = self.recvData().await?;
        let request = lib::client::decode(&data)?;
        Ok(request)
    }

    pub async fn start(&mut self) -> Result<String> {
        let request = self.recvRequest().await?;
        if let lib::client::Message::Open(open) = request {
            return Ok(open.hostname.to_owned());
        }

        Err(Error::UnexpectedMessage.into())
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        //let request = self.recvRequest().await?;
        Ok(())
    }

    pub async fn send(&self, resp: &lib::server::Message) -> Result<()> {
        println!("begin send");
        let encoded = lib::server::encode(resp)?;
        let message = Message::Binary(encoded);
        let mut sender = self.ws_sender.lock().await;
        sender.send(message).await?;
        println!("end send");
        Ok(())
    }

    /// helper to print contents of messages to stdout. Has special treatment for Close.
    fn process_message(&self, msg: axum::extract::ws::Message) -> ControlFlow<(), ()> {
        match msg {
            Message::Text(t) => {
                println!(">>> {} sent str: {:?}", self.who, t);
            }
            Message::Binary(d) => {
                println!(">>> {} sent {} bytes: {:?}", self.who, d.len(), d);
            }
            Message::Close(c) => {
                if let Some(cf) = c {
                    println!(
                        ">>> {} sent close with code {} and reason `{}`",
                        self.who, cf.code, cf.reason
                    );
                } else {
                    println!(
                        ">>> {} somehow sent close message without CloseFrame",
                        self.who
                    );
                }
                return ControlFlow::Break(());
            }

            Message::Pong(v) => {
                println!(">>> {} sent pong with {:?}", self.who, v);
            }
            // You should never need to manually handle Message::Ping, as axum's websocket library
            // will do so for you automagically by replying with Pong and copying the v according to
            // spec. But if you need the contents of the pings you can see them here.
            Message::Ping(v) => {
                println!(">>> {} sent ping with {:?}", self.who, v);
            }
        }
        ControlFlow::Continue(())
    }
}
