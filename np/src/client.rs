use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::SinkExt;
use futures_util::StreamExt;

use libnp::server::split_origin;
use libnp::Addr;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use libnp::common::PortMessage;
use libnp::PortProtocol;

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
    sender: Arc<RwLock<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>,
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
        Ok(())
    }

    pub async fn http_open(&self, hostname: &str, local_addr: &Addr) -> Result<()> {
        let msg = libnp::client::Message::HttpOpen(libnp::client::HttpOpen {
            hostname: hostname.to_owned(),
            local_addr: local_addr.clone(),
        });
        self.send(&msg).await
    }

    pub async fn http_register(&self, msg: &libnp::server::HttpOpened) -> Result<()> {
        let hostname = msg.hostname.clone();
        if let Some(error) = msg.error {
            return Err(Error::HttpOpenError(hostname, error));
        };
        let mut http_forwardings = self.http_forwardings.write().await;
        if let Entry::Vacant(entry) = http_forwardings.entry(hostname.clone()) {
            entry.insert(msg.local_addr.clone());
            Ok(())
        } else {
            Err(Error::HttpHostnameAlreadyRegistered(hostname))
        }
    }

    pub async fn http_deregister(&self, msg: &libnp::server::HttpClosed) -> Result<()> {
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
        let msg = libnp::client::Message::PortOpen(libnp::client::PortOpen {
            protocol,
            remote_addr: remote_addr.clone(),
            local_addr: local_addr.clone(),
        });
        self.send(&msg).await
    }

    pub async fn port_register(&self, msg: &libnp::server::PortOpened) -> Result<()> {
        if let Some(error) = msg.error {
            return Err(Error::PortOpenError(msg.remote_addr.clone(), error));
        };
        let origin = msg.origin();
        let mut port_forwardings = self.port_forwardings.write().await;
        if let Entry::Vacant(entry) = port_forwardings.entry(origin.clone()) {
            entry.insert(msg.local_addr.clone());
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
        msg: &libnp::server::PortConnect,
        f: F,
    ) -> Result<()>
    where
        F: FnOnce(String, Addr) -> Future,
        Future: std::future::Future<Output = Result<()>>,
    {
        let origin = msg.origin();
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
