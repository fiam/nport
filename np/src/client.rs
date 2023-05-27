use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use libnp::common::PortMessage;
use libnp::PortProtocol;

use crate::error::{Error, Result};

const SERVER: &str = "ws://127.0.0.1:3000/v1/connect";

pub mod port;

struct Connection {
    sender: Arc<RwLock<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
    receiver: Arc<RwLock<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}

pub struct Client {
    // hostname => local_port
    http_forwardings: RwLock<HashMap<String, u16>>,
    // protocol:hostname => local port
    port_forwardings: RwLock<HashMap<String, u16>>,
    // port uuid => queue
    port_writers: RwLock<HashMap<String, mpsc::Sender<PortMessage>>>,
    connection: RwLock<Option<Connection>>,
}

impl Client {
    pub fn new() -> Self {
        Self {
            http_forwardings: RwLock::new(HashMap::new()),
            port_forwardings: RwLock::new(HashMap::new()),
            port_writers: RwLock::new(HashMap::new()),
            connection: RwLock::new(None),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let (stream, response) = connect_async(SERVER).await?;
        tracing::debug!(response=?response, "connected to server");
        let (sender, receiver) = stream.split();
        let connection = Connection {
            sender: Arc::new(RwLock::new(sender)),
            receiver: Arc::new(RwLock::new(receiver)),
        };
        *self.connection.write().await = Some(connection);
        Ok(())
    }

    pub async fn http_open(&self, hostname: &str, local_port: u16) -> Result<()> {
        let msg = libnp::client::Message::HttpOpen(libnp::client::HttpOpen {
            hostname: hostname.to_owned(),
            local_port,
        });
        self.send(&msg).await
    }

    pub async fn http_register(&self, msg: &libnp::server::HttpOpened) -> Result<()> {
        let hostname = msg.hostname.clone();
        let mut http_forwardings = self.http_forwardings.write().await;
        if let Entry::Vacant(entry) = http_forwardings.entry(hostname.clone()) {
            entry.insert(msg.local_port);
            return Ok(());
        }
        Err(Error::HttpHostnameAlreadyRegistered(hostname))
    }

    pub async fn http_deregister(&self, msg: &libnp::server::HttpClosed) -> Result<()> {
        let mut http_forwardings = self.http_forwardings.write().await;
        if let Entry::Occupied(entry) = http_forwardings.entry(msg.hostname.clone()) {
            entry.remove();
            return Ok(());
        }
        Err(Error::HttpHostnameNotRegistered(msg.hostname.clone()))
    }

    pub async fn http_port(&self, hostname: &str) -> Option<u16> {
        self.http_forwardings.read().await.get(hostname).copied()
    }

    pub async fn tcp_open(&self, hostname: &str, port: u16, local_port: u16) -> Result<()> {
        self.port_open(PortProtocol::Tcp, hostname, port, local_port)
            .await
    }

    pub async fn port_open(
        &self,
        protocol: PortProtocol,
        hostname: &str,
        port: u16,
        local_port: u16,
    ) -> Result<()> {
        let msg = libnp::client::Message::PortOpen(libnp::client::PortOpen {
            protocol,
            hostname: hostname.to_owned(),
            port,
            local_port,
        });
        self.send(&msg).await
    }

    pub async fn port_register(&self, msg: &libnp::server::PortOpened) -> Result<()> {
        let origin = msg.origin();
        let mut port_forwardings = self.port_forwardings.write().await;
        if let Entry::Vacant(entry) = port_forwardings.entry(origin.clone()) {
            entry.insert(msg.local_port);
            return Ok(());
        }
        Err(Error::PortOriginAlreadyRegistered(origin))
    }

    pub async fn port_writer_register(
        &self,
        uuid: &str,
        sender: mpsc::Sender<PortMessage>,
    ) -> Result<()> {
        tracing::trace!(uuid, "registering port writer");
        let uuid = uuid.to_string();
        let mut port_writers = self.port_writers.write().await;
        if let Entry::Vacant(entry) = port_writers.entry(uuid.clone()) {
            entry.insert(sender);
            Ok(())
        } else {
            Err(Error::PortIDAlreadyRegistered(uuid.clone()))
        }
    }

    pub async fn port_writer_remove(&self, uuid: &str) -> Result<()> {
        tracing::trace!(uuid, "removing port writer");
        self.port_writers.write().await.remove(uuid);
        Ok(())
    }

    pub async fn port_connect<F, Future>(
        &self,
        msg: &libnp::server::PortConnect,
        f: F,
    ) -> Result<()>
    where
        F: FnOnce(String, String) -> Future,
        Future: std::future::Future<Output = Result<()>>,
    {
        let origin = msg.origin();
        let local_port = match self.port_forwardings.read().await.get(&origin) {
            None => return Err(Error::PortOriginNotRegistered(origin)),
            Some(port) => *port,
        };
        let addr = format!("127.0.0.1:{local_port}");
        f(msg.uuid.clone(), addr).await
    }

    async fn port_message(&self, uuid: &str, msg: PortMessage) -> Result<()> {
        let port_writers = self.port_writers.read().await;
        let writer = port_writers
            .get(uuid)
            .ok_or(Error::PortIDNotRegistered(uuid.to_string()))?;
        if let Err(error) = writer.send(msg).await {
            tracing::error!(error = error.to_string(), "sending port message");
            return Err(Error::PortIDNotRegistered(uuid.to_string()));
        }
        Ok(())
    }

    pub async fn port_receive(&self, msg: &libnp::server::PortReceive) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Data(msg.data.clone()))
            .await
    }

    pub async fn port_close(&self, msg: &libnp::server::PortClose) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Close).await
    }

    pub async fn send(&self, msg: &libnp::client::Message) -> Result<()> {
        let connection = self.connection.read().await;
        match connection.as_ref() {
            Some(connection) => {
                let encoded = libnp::client::encode(msg)?;
                let encoded_msg = Message::Binary(encoded);
                let mut sender = connection.sender.write().await;
                Ok(sender.send(encoded_msg).await?)
            }
            None => Err(Error::Disconnected),
        }
    }

    pub async fn recv(&self) -> Result<libnp::server::Message> {
        let connection = self.connection.read().await;
        match connection.as_ref() {
            Some(connection) => {
                let mut receiver = connection.receiver.write().await;
                let received = receiver.next().await.ok_or(Error::Disconnected)??;
                match received {
                    Message::Binary(data) => Ok(libnp::server::decode(&data)?),
                    _ => Err(Error::InvalidMessageType),
                }
            }
            None => Err(Error::Disconnected),
        }
    }
}
