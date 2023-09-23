use std::{collections::HashMap, fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json;
use url::Url;

use crate::{error::Result, Addr, Error, PortProtocol};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum HttpOpenError {
    /// Hostname is being used by another client
    InUse,
    /// Hostname is not valid (e.g. contains a dot)
    Invalid,
    /// Hostname is disallowed (e.g. part of the blocklist)
    Disallowed,
}

impl fmt::Display for HttpOpenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let message = match self {
            HttpOpenError::InUse => "hostname is already in use",
            HttpOpenError::Invalid => "hostname is not valid",
            HttpOpenError::Disallowed => "hostname is not allowed",
        };
        write!(f, "{}", message)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpOpened {
    pub hostname: String,
    pub local_addr: Addr,
    pub error: Option<HttpOpenError>,
}

impl HttpOpened {
    pub fn ok(hostname: &str, local_addr: &Addr) -> Self {
        Self {
            hostname: hostname.to_owned(),
            local_addr: local_addr.clone(),
            error: None,
        }
    }

    pub fn error(hostname: &str, local_addr: &Addr, error: HttpOpenError) -> Self {
        Self {
            hostname: hostname.to_string(),
            local_addr: local_addr.clone(),
            error: Some(error),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpCloseError {
    // Hostname was not registered
    NotRegistered,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpClosed {
    pub hostname: String,
    pub error: Option<HttpCloseError>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpRequest {
    pub uuid: String,
    pub hostname: String,
    pub addr: String,
    pub protocol: String,
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, Vec<u8>>,
    pub body: Vec<u8>,
}

fn port_origin(protocol: PortProtocol, addr: &Addr) -> String {
    format!("{protocol}://{}", addr)
}

pub fn split_origin(origin: &str) -> Result<(PortProtocol, String, u16)> {
    let url = Url::parse(origin).map_err(|e| Error::InvalidOrigin(format!("{}: {}", origin, e)))?;
    let protocol =
        PortProtocol::from_str(url.scheme()).map_err(|e| Error::InvalidOrigin(e.to_string()))?;
    let host = url.host_str().ok_or(Error::InvalidOrigin(format!(
        "origin {} is missing a host",
        origin
    )))?;
    let port = url.port().ok_or(Error::InvalidOrigin(format!(
        "origin {} is missing a port",
        origin
    )))?;
    Ok((protocol, host.to_string(), port))
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum PortOpenError {
    /// Port or host:port is already in use
    InUse,
    // Port is not allowed
    PortNotAllowed,
    // Host is not allowed
    HostNotAllowed,
}

impl std::fmt::Display for PortOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            PortOpenError::InUse => "host/port is already in use",
            PortOpenError::PortNotAllowed => "port is not allowed",
            PortOpenError::HostNotAllowed => "host is not allowed",
        };
        write!(f, "{}", message)
    }
}

/// Sent when the server successfully creates a port forwarding
#[derive(Serialize, Deserialize, Debug)]
pub struct PortOpened {
    /// Protocol used by the port that was opened
    pub protocol: PortProtocol,
    /// Remote address opened for forwarding. Note that this won't necessarily
    /// match the address specified by the client (e.g. client requested port 0
    /// and the server assigned a random one)
    pub remote_addr: Addr,
    /// local address that the client will forward this port to
    pub local_addr: Addr,
    /// Error when opening the port (if any)
    pub error: Option<PortOpenError>,
}

impl PortOpened {
    pub fn origin(&self) -> String {
        port_origin(self.protocol, &self.remote_addr)
    }
}

/// Sent when a client is trying to connect the the remote address
/// of the tunnel
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortConnect {
    /// uuid which uniquely identifies this client connection. The same uuid
    /// is then when transmitting PortReceive and PortClose.
    pub uuid: String,
    /// Protocol used by the tunnel
    pub protocol: PortProtocol,
    /// Remote address (i.e. the tunnel entry managed by the server)
    pub remote: Addr,
    /// Client address that's trying to connect to the port
    pub from: Addr,
}

impl PortConnect {
    pub fn origin(&self) -> String {
        port_origin(self.protocol, &self.remote)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortReceive {
    pub uuid: String,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortClose {
    pub uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    HttpOpened(HttpOpened),
    HttpClosed(HttpClosed),
    HttpRequest(HttpRequest),
    PortOpened(PortOpened),
    PortConnect(PortConnect),
    PortReceive(PortReceive),
    PortClose(PortClose),
}

pub fn decode(data: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
