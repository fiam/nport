use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;

use libnp::messages;
use libnp::messages::common::PortProtocol;
use libnp::port_origin_from_raw;
use libnp::split_origin;
use libnp::Addr;
use prost::Message;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use libnp::common::PortMessage;

use crate::error::HttpOpenErrorMessage;
use crate::error::PortOpenErrorMessage;
use crate::error::{Error, Result};

pub mod port;

#[derive(Debug)]
pub struct HttpForwarding {
    hostname: String,
    local_addr: Addr,
}

impl HttpForwarding {
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn local_addr(&self) -> &Addr {
        &self.local_addr
    }
}

#[derive(Debug)]
pub struct PortForwarding {
    protocol: PortProtocol,
    remote_addr: Addr,
    local_addr: Addr,
}

impl PortForwarding {
    pub fn protocol(&self) -> PortProtocol {
        self.protocol
    }
    pub fn remote_addr(&self) -> &Addr {
        &self.remote_addr
    }
    pub fn local_addr(&self) -> &Addr {
        &self.local_addr
    }
}

struct Connection {
    sender:
        Arc<RwLock<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>>>,
    receiver: Arc<RwLock<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}

#[derive(Clone)]
pub struct VersionInfo {
    pkg_version: String,
    git_ref: String,
    git_is_dirty: bool,
}

impl VersionInfo {
    pub fn new(pkg_version: &str, git_ref: &str, git_is_dirty: bool) -> Self {
        Self {
            pkg_version: pkg_version.to_string(),
            git_ref: git_ref.to_string(),
            git_is_dirty,
        }
    }
}

pub struct Client {
    version_info: VersionInfo,
    // hostname => local_addr
    http_forwardings: RwLock<HashMap<String, Addr>>,
    // protocol:hostname:remote_port => local addr
    port_forwardings: RwLock<HashMap<String, Addr>>,
    // port uuid => queue
    port_writers: RwLock<HashMap<String, mpsc::Sender<PortMessage>>>,
    connection: RwLock<Option<Connection>>,
}

impl Client {
    pub fn new(version_info: VersionInfo) -> Self {
        Self {
            version_info,
            http_forwardings: RwLock::new(HashMap::new()),
            port_forwardings: RwLock::new(HashMap::new()),
            port_writers: RwLock::new(HashMap::new()),
            connection: RwLock::new(None),
        }
    }

    pub async fn connect(&self, server: &str, secure: bool) -> Result<()> {
        let protocol = if secure { "wss" } else { "ws" };
        let server_url = format!(
            "{protocol}://{server}/v1/connect?v={}&ref={}&dirty={}&os={}",
            self.version_info.pkg_version,
            self.version_info.git_ref,
            self.version_info.git_is_dirty,
            std::env::consts::OS,
        );
        let (stream, response) = connect_async(server_url).await?;
        tracing::debug!(response=?response, "connected to server");
        let (sender, receiver) = stream.split();
        let connection = Connection {
            sender: Arc::new(RwLock::new(sender)),
            receiver: Arc::new(RwLock::new(receiver)),
        };
        *self.connection.write().await = Some(connection);
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        *self.connection.write().await = None;
        self.http_forwardings.write().await.clear();
        self.port_forwardings.write().await.clear();
        self.port_writers.write().await.clear();
        Ok(())
    }

    pub async fn http_open(&self, hostname: &str, local_addr: &Addr) -> Result<()> {
        let msg = messages::client::payload::Message::HttpOpen(messages::client::HttpOpen {
            hostname: Some(hostname.to_string()),
            local_address: Some(local_addr.to_address()),
        });
        self.send(msg).await
    }

    pub async fn http_register(&self, msg: &messages::server::HttpOpened) -> Result<()> {
        let hostname = msg.hostname.clone();
        if let Some(error) = msg.error {
            return Err(Error::HttpOpenError(hostname, HttpOpenErrorMessage(error)));
        };
        let mut http_forwardings = self.http_forwardings.write().await;
        if let Entry::Vacant(entry) = http_forwardings.entry(hostname.clone()) {
            let addr = Addr::from_address(msg.local_address.as_ref());
            entry.insert(addr);
            Ok(())
        } else {
            Err(Error::HttpHostnameAlreadyRegistered(hostname))
        }
    }

    pub async fn http_deregister(&self, msg: &messages::server::HttpClosed) -> Result<()> {
        let mut http_forwardings = self.http_forwardings.write().await;
        if let Entry::Occupied(entry) = http_forwardings.entry(msg.hostname.clone()) {
            entry.remove();
            return Ok(());
        }
        Err(Error::HttpHostnameNotRegistered(msg.hostname.clone()))
    }

    pub async fn http_port(&self, hostname: &str) -> Option<Addr> {
        self.http_forwardings.read().await.get(hostname).cloned()
    }

    pub async fn tcp_open(&self, remote_addr: &Addr, local_addr: &Addr) -> Result<()> {
        self.port_open(PortProtocol::Tcp, remote_addr, local_addr)
            .await
    }

    pub async fn port_open(
        &self,
        protocol: PortProtocol,
        remote_addr: &Addr,
        local_addr: &Addr,
    ) -> Result<()> {
        let msg = messages::client::payload::Message::PortOpen(messages::client::PortOpen {
            protocol: protocol as i32,
            remote_address: Some(remote_addr.to_address()),
            local_address: Some(local_addr.to_address()),
        });
        self.send(msg).await
    }

    pub async fn port_register(&self, msg: &messages::server::PortOpened) -> Result<()> {
        if let Some(error) = msg.error {
            return Err(Error::PortOpenError(
                Addr::from_address(msg.remote_address.as_ref()),
                PortOpenErrorMessage(error),
            ));
        };
        let origin = port_origin_from_raw(msg.protocol, msg.remote_address.as_ref());
        let mut port_forwardings = self.port_forwardings.write().await;
        if let Entry::Vacant(entry) = port_forwardings.entry(origin.clone()) {
            entry.insert(Addr::from_address(msg.local_address.as_ref()));
            Ok(())
        } else {
            Err(Error::PortOriginAlreadyRegistered(origin))
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
        msg: &messages::server::PortConnect,
        f: F,
    ) -> Result<()>
    where
        F: FnOnce(String, Addr) -> Future,
        Future: std::future::Future<Output = Result<()>>,
    {
        let origin = port_origin_from_raw(msg.protocol, msg.remote_address.as_ref());
        let Some(local_addr) = self.port_forwardings.read().await.get(&origin).cloned() else {
            return Err(Error::PortOriginNotRegistered(origin));
        };
        f(msg.uuid.clone(), local_addr).await
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

    pub async fn port_receive(&self, msg: &messages::server::PortReceive) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Data(msg.data.clone()))
            .await
    }

    pub async fn port_close(&self, msg: &messages::server::PortClose) -> Result<()> {
        self.port_message(&msg.uuid, PortMessage::Close).await
    }

    pub async fn send(&self, msg: libnp::messages::client::payload::Message) -> Result<()> {
        let conn = self.connection.read().await;
        let Some(connection) = conn.as_ref() else {
            return Err(Error::Disconnected);
        };
        let payload = messages::client::Payload { message: Some(msg) };
        let mut buf = Vec::with_capacity(payload.encoded_len());
        payload.encode(&mut buf)?;
        let message = tungstenite::Message::Binary(buf);
        let mut sender = connection.sender.write().await;
        Ok(sender.send(message).await?)
    }

    pub async fn recv(&self) -> Result<messages::server::payload::Message> {
        let conn = self.connection.read().await;
        let Some(connection) = conn.as_ref() else {
            return Err(Error::Disconnected);
        };
        let mut receiver = connection.receiver.write().await;
        let received = receiver.next().await.ok_or(Error::Disconnected)??;
        let tungstenite::Message::Binary(data) = received else {
            return Err(Error::InvalidMessageType);
        };
        messages::server::Payload::decode(&mut Cursor::new(data))
            .map(|p| p.message.ok_or(Error::MalformedMessage))?
    }

    pub async fn http_forwardings(&self) -> Vec<HttpForwarding> {
        let mut forwardings = vec![];
        self.http_forwardings.read().await.iter().for_each(|entry| {
            forwardings.push(HttpForwarding {
                hostname: entry.0.clone(),
                local_addr: entry.1.clone(),
            })
        });
        forwardings
    }

    pub async fn port_forwardings(&self) -> Vec<PortForwarding> {
        let mut forwardings = vec![];
        self.port_forwardings.read().await.iter().for_each(|entry| {
            let (protocol, hostname, port) = split_origin(entry.0).unwrap();
            forwardings.push(PortForwarding {
                protocol,
                remote_addr: Addr::from_host_and_port(&hostname, port),
                local_addr: entry.1.clone(),
            });
        });
        forwardings
    }
}
