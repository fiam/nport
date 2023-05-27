use std::collections::HashMap;
use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::stream::SplitStream;
use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::sync::{oneshot, Mutex};

use lib::common::PortMessage;
use liblocalport as lib;

use super::port_server;

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
    #[error("duplicate port with uuid {0}")]
    PortIDAlreadyRegistered(String),
    #[error("no port with uuid {0}")]
    PortIDNotRegistered(String),
}

pub struct Client {
    hostnames: RwLock<HashSet<String>>,
    ws_sender: Mutex<SplitSink<WebSocket, Message>>,
    ws_receiver: Mutex<SplitStream<WebSocket>>,
    http_requests: Mutex<HashMap<String, oneshot::Sender<lib::client::HttpResponse>>>,
    connect_requests: Mutex<HashMap<String, oneshot::Sender<lib::client::PortConnected>>>,
    open_ports: Mutex<Vec<port_server::Port>>,
    port_writers: RwLock<HashMap<String, mpsc::Sender<PortMessage>>>,
    who: SocketAddr,
}

impl Client {
    pub fn new(socket: WebSocket, who: SocketAddr) -> Client {
        let (ws_sender, ws_receiver) = socket.split();
        Client {
            hostnames: RwLock::new(HashSet::new()),
            ws_sender: Mutex::new(ws_sender),
            ws_receiver: Mutex::new(ws_receiver),
            http_requests: Mutex::new(HashMap::new()),
            connect_requests: Mutex::new(HashMap::new()),
            open_ports: Mutex::new(Vec::new()),
            port_writers: RwLock::new(HashMap::new()),
            who,
        }
    }

    pub fn who(&self) -> &SocketAddr {
        &self.who
    }

    pub async fn add_http_hostname(&self, hostname: &str) -> bool {
        self.hostnames.write().await.insert(hostname.to_owned())
    }

    pub async fn remove_http_hostname(&self, hostname: &str) -> bool {
        self.hostnames.write().await.remove(hostname)
    }

    pub async fn http_hostnames(&self) -> Vec<String> {
        let result: Vec<String> = self.hostnames.read().await.iter().cloned().collect();
        result
    }

    pub async fn has_http_hostname(&self, hostname: &str) -> bool {
        self.hostnames.read().await.contains(hostname)
    }

    pub async fn register_http_request(
        &self,
        id: &str,
    ) -> oneshot::Receiver<lib::client::HttpResponse> {
        let (tx, rx) = oneshot::channel::<lib::client::HttpResponse>();
        self.http_requests.lock().await.insert(id.to_string(), tx);
        rx
    }

    pub async fn send_http_response(&self, response: lib::client::HttpResponse) -> Result<()> {
        let tx = self.http_requests.lock().await.remove(&response.uuid);
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

    pub async fn register_connect_request(
        &self,
        id: &str,
    ) -> oneshot::Receiver<lib::client::PortConnected> {
        let (tx, rx) = oneshot::channel::<lib::client::PortConnected>();
        self.connect_requests
            .lock()
            .await
            .insert(id.to_string(), tx);
        rx
    }

    pub async fn send_connected_response(
        &self,
        connected: lib::client::PortConnected,
    ) -> Result<()> {
        let tx = self.connect_requests.lock().await.remove(&connected.uuid);
        match tx {
            Some(tx) => {
                _ = tx.send(connected);
            }
            None => {
                return Err(Error::NoRequest.into());
            }
        }
        Ok(())
    }

    pub async fn register_port(&self, port: port_server::Port) {
        self.open_ports.lock().await.push(port);
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

    pub async fn port_writer_register(
        &self,
        uuid: &str,
        sender: mpsc::Sender<PortMessage>,
    ) -> Result<()> {
        tracing::trace!(uuid, "registering port writer");
        let uuid = uuid.to_string();
        let mut port_writers = self.port_writers.write().await;
        if let std::collections::hash_map::Entry::Vacant(entry) = port_writers.entry(uuid.clone()) {
            entry.insert(sender);
            Ok(())
        } else {
            Err(Error::PortIDAlreadyRegistered(uuid.clone()).into())
        }
    }

    pub async fn port_writer_remove(&self, uuid: &str) -> Result<()> {
        tracing::trace!(uuid, "removing port writer");
        self.port_writers.write().await.remove(uuid);
        Ok(())
    }

    async fn port_message(&self, uuid: &str, msg: PortMessage) -> Result<()> {
        let port_writers = self.port_writers.read().await;
        let writer = port_writers
            .get(uuid)
            .ok_or(Error::PortIDNotRegistered(uuid.to_string()))?;
        if let Err(error) = writer.send(msg).await {
            tracing::error!(error = error.to_string(), "sending to write");
            return Err(Error::PortIDNotRegistered(uuid.to_string()).into());
        }
        Ok(())
    }

    pub async fn port_receive(&self, msg: &lib::client::PortReceive) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Data(msg.data.clone()))
            .await
    }

    pub async fn port_close(&self, msg: &lib::client::PortClose) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Close).await
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

    // pub async fn open_tcp(&self, hostname: &str, port: u16) -> Result<u16> {
    //     Ok(0)
    // }
}
