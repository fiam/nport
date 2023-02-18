use std::collections::HashMap;
use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::stream::SplitStream;
use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::sync::{oneshot, Mutex};

use liblocalport as lib;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("client disconnected")]
    Disconnected,
    #[error("client not found")]
    ClientNotFound,
    #[error("unexpected client message type")]
    UnexpectedMessageType,
    #[error("no request for response")]
    NoRequest,
}

pub struct Client {
    hostname: String,
    ws_sender: Mutex<SplitSink<WebSocket, Message>>,
    ws_receiver: Mutex<SplitStream<WebSocket>>,
    requests: Mutex<HashMap<String, Sender<lib::client::HttpResponse>>>,
    who: SocketAddr,
}

impl Client {
    pub fn new(socket: WebSocket, who: SocketAddr) -> Client {
        let (ws_sender, ws_receiver) = socket.split();
        Client {
            hostname: "".to_owned(),
            ws_sender: Mutex::new(ws_sender),
            ws_receiver: Mutex::new(ws_receiver),
            requests: Mutex::new(HashMap::new()),
            who,
        }
    }

    pub fn is_open(&self) -> bool {
        !self.hostname.is_empty()
    }

    pub fn set_hostname(&mut self, hostname: &str) {
        self.hostname = hostname.to_owned();
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub async fn register(&self, id: &str) -> Receiver<lib::client::HttpResponse> {
        let (tx, rx) = oneshot::channel::<lib::client::HttpResponse>();
        self.requests.lock().await.insert(id.to_string(), tx);
        rx
    }

    pub async fn send_response(&self, response: lib::client::HttpResponse) -> Result<()> {
        let tx = self.requests.lock().await.remove(&response.uuid);
        match tx {
            Some(tx) => {
                _ = tx.send(response);
            }
            None => {
                return Err(Error::NoRequest.into());
            }
        }
        Ok(())
    }

    async fn recv_data(&self) -> Result<Vec<u8>> {
        let mut receiver = self.ws_receiver.lock().await;
        match receiver.next().await {
            Some(received) => match received? {
                Message::Binary(data) => Ok(data),
                _ => Err(Error::UnexpectedMessageType.into()),
            },
            None => Err(Error::Disconnected.into()),
        }
    }

    pub async fn recv(&self) -> Result<lib::client::Message> {
        let data = self.recv_data().await?;
        let request = lib::client::decode(&data)?;
        Ok(request)
    }

    pub async fn send(&self, resp: &lib::server::Message) -> Result<()> {
        let encoded = lib::server::encode(resp)?;
        let message = Message::Binary(encoded);
        let mut sender = self.ws_sender.lock().await;
        sender.send(message).await?;
        Ok(())
    }
}
