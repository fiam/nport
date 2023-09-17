use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_json;

use crate::{error::Result, Addr, Error, PortProtocol};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpOpenResult {
    Ok,
    InUse,
    Invalid,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpOpened {
    pub hostname: String,
    pub local_addr: Addr,
    pub result: HttpOpenResult,
}

impl HttpOpened {
    pub fn ok(hostname: &str, local_addr: &Addr) -> Self {
        Self {
            hostname: hostname.to_owned(),
            local_addr: local_addr.clone(),
            result: HttpOpenResult::Ok,
        }
    }

    pub fn failed(hostname: &str, local_addr: &Addr, result: HttpOpenResult) -> Self {
        assert!(result != HttpOpenResult::Ok);
        Self {
            hostname: hostname.to_string(),
            local_addr: local_addr.clone(),
            result,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpCloseResult {
    Ok,
    NotRegistered,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpClosed {
    pub hostname: String,
    pub result: HttpCloseResult,
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

fn port_origin(protocol: PortProtocol, hostname: &str, port: u16) -> String {
    format!("{protocol}:{hostname}:{port}")
}

pub fn split_origin(origin: &str) -> Result<(PortProtocol, String, u16)> {
    let parts = origin.split(':').collect::<Vec<&str>>();
    if parts.len() != 3 {
        return Err(Error::InvalidOrigin(format!(
            "'{}' doesn't have 3 segments",
            origin
        )));
    }
    let protocol =
        PortProtocol::from_str(parts[0]).map_err(|e| Error::InvalidOrigin(e.to_string()))?;
    let port = parts[2]
        .parse::<u16>()
        .map_err(|e| Error::InvalidOrigin(format!("port {} is not a number: {}", parts[2], e)))?;
    Ok((protocol, parts[1].to_owned(), port))
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum PortOpenResult {
    Ok,
    // Port or host:port is already in use
    PortInUse,
    // Port is not allowed
    PortNotAllowed,
    // Host is not allowed
    HostNotAllowed,
}

impl std::fmt::Display for PortOpenResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortOpenResult::Ok => write!(f, "ok"),
            PortOpenResult::PortInUse => write!(f, "port is already in use"),
            PortOpenResult::PortNotAllowed => write!(f, "port is not allowed"),
            PortOpenResult::HostNotAllowed => write!(f, "hostname is not allowed"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PortOpened {
    pub protocol: PortProtocol,
    pub hostname: String,
    pub port: u16,
    pub local_addr: Addr,
    pub result: PortOpenResult,
}

impl PortOpened {
    pub fn origin(&self) -> String {
        port_origin(self.protocol, &self.hostname, self.port)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortConnect {
    pub uuid: String,
    pub protocol: PortProtocol,
    pub hostname: String,
    pub port: u16,
    pub from: String,
}

impl PortConnect {
    pub fn origin(&self) -> String {
        port_origin(self.protocol, &self.hostname, self.port)
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
