use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio::sync::RwLock;

use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use liblocalport as lib;

use crate::error::{Error, Result};

const SERVER: &'static str = "ws://127.0.0.1:3000/v1/connect";

struct Connection {
    sender: Arc<RwLock<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    receiver: Arc<RwLock<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}

pub struct Client {
    http_forwardings: RwLock<HashMap<String, u16>>,
    connection: RwLock<Option<Connection>>,
}

impl Client {
    pub fn new() -> Self {
        Self {
            http_forwardings: RwLock::new(HashMap::new()),
            connection: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let (stream, response) = connect_async(SERVER).await?;
        println!("server response {:?}", response);
        let (sender, receiver) = stream.split();
        let connection = Connection {
            sender: Arc::new(RwLock::new(sender)),
            receiver: Arc::new(RwLock::new(receiver)),
        };
        *self.connection.write().await = Some(connection);
        Ok(())
    }

    pub async fn http_open(&self, hostname: &str, local_port: u16) -> Result<()> {
        let msg = lib::client::Message::HttpOpen(lib::client::HttpOpen {
            hostname: hostname.to_owned(),
            local_port,
        });
        self.send(&msg).await
    }

    pub async fn http_register(&self, msg: &lib::server::HttpOpen) -> Result<()> {
        let hostname = msg.hostname.clone();
        let mut http_forwardings = self.http_forwardings.write().await;
        if http_forwardings.insert(hostname, msg.local_port).is_none() {
            Err(Error::HttpHostnameAlreadyRegistered(msg.hostname.clone()))
        } else {
            Ok(())
        }
    }

    pub async fn http_deregister(&self, msg: &lib::server::HttpClose) -> Result<()> {
        let mut http_forwardings = self.http_forwardings.write().await;
        if http_forwardings.remove(&msg.hostname).is_none() {
            Err(Error::HttpHostnameNotRegistered(msg.hostname.clone()))
        } else {
            Ok(())
        }
    }

    pub async fn http_port(&self, hostname: &str) -> Option<u16> {
        self.http_forwardings.read().await.get(hostname).copied()
    }

    pub async fn tcp_open(&self, hostname: &str, port: u16, local_port: u16) -> Result<()> {
        let msg = lib::client::Message::TcpOpen(lib::client::TcpOpen {
            hostname: hostname.to_owned(),
            port,
            local_port,
        });
        self.send(&msg).await
    }

    pub async fn send(&self, msg: &lib::client::Message) -> Result<()> {
        let connection = self.connection.read().await;
        match connection.as_ref() {
            Some(connection) => {
                let encoded = lib::client::encode(msg)?;
                let encoded_msg = Message::Binary(encoded);
                let mut sender = connection.sender.write().await;
                return Ok(sender.send(encoded_msg).await?);
            }
            None => Err(Error::Disconnected),
        }
    }

    pub async fn recv(&self) -> Result<lib::server::Message> {
        let connection = self.connection.read().await;
        match connection.as_ref() {
            Some(connection) => {
                let mut receiver = connection.receiver.write().await;
                let received = receiver.next().await.ok_or(Error::Disconnected)??;
                match received {
                    Message::Binary(data) => Ok(lib::server::decode(&data)?),
                    _ => Err(Error::InvalidMessageType),
                }
            }
            None => Err(Error::Disconnected),
        }
    }
}
