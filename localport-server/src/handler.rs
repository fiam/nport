use std::{net::SocketAddr, ops::ControlFlow};

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::stream::SplitStream;
use futures::SinkExt;
use futures::StreamExt;
use thiserror;

use liblocalport::client;
use liblocalport::server;

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
    ws_sender: SplitSink<WebSocket, Message>,
    ws_receiver: SplitStream<WebSocket>,
    who: SocketAddr,
}

impl Handler {
    pub fn new(socket: WebSocket, who: SocketAddr) -> Handler {
        let (ws_sender, ws_receiver) = socket.split();
        Handler {
            hostname: "".to_owned(),
            ws_sender,
            ws_receiver,
            who,
        }
    }

    pub fn is_open(&self) -> bool {
        self.hostname.len() > 0
    }

    pub fn set_hostname(&mut self, hostname: &str) {
        self.hostname = hostname.to_owned();
    }

    async fn recvData(&mut self) -> Result<Vec<u8>> {
        match self.ws_receiver.next().await {
            Some(received) => match received? {
                Message::Binary(data) => Ok(data),
                _ => Err(Error::UnexpectedMessageType.into()),
            },
            None => Err(Error::Disconnected.into()),
        }
    }

    pub async fn recvRequest(&mut self) -> Result<client::request::Request> {
        let data = self.recvData().await?;
        let request = client::request::decode(&data)?;
        Ok(request)
    }

    pub async fn start(&mut self) -> Result<String> {
        let request = self.recvRequest().await?;
        if let client::request::Request::Open(open) = request {
            return Ok(open.hostname.to_owned());
        }

        Err(Error::UnexpectedMessage.into())
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        //let request = self.recvRequest().await?;
        Ok(())
    }

    pub async fn send(&mut self, resp: &server::response::Response) -> Result<()> {
        if let Err(err) = self
            .ws_sender
            .send(Message::Binary(server::response::encode(resp).unwrap()))
            .await
        {
            Err(err.into())
        } else {
            Ok(())
        }
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
