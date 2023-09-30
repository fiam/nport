use std::fmt;

use libnp::{
    messages::server::{HttpOpenError, PortOpenError},
    Addr,
};
use thiserror::Error;

#[derive(Debug)]
/// Wraps HttpOpenError
pub struct HttpOpenErrorMessage(pub i32);

impl fmt::Display for HttpOpenErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message: String = if let Ok(error) = HttpOpenError::try_from(self.0) {
            match error {
                HttpOpenError::InUse => "hostname is already in use",
                HttpOpenError::Invalid => "hostname is not valid",
                HttpOpenError::Disallowed => "hostname is not allowed",
            }
            .to_string()
        } else {
            format!("unknown HttpOpenError {}", self.0)
        };
        write!(f, "{}", message)
    }
}

#[derive(Debug)]
// Wraps PortOpenError
pub struct PortOpenErrorMessage(pub i32);

impl fmt::Display for PortOpenErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message: String = if let Ok(error) = PortOpenError::try_from(self.0) {
            match error {
                PortOpenError::InUse => "host/port is already in use",
                PortOpenError::PortNotAllowed => "port is not allowed",
                PortOpenError::HostNotAllowed => "host is not allowed",
            }
            .to_string()
        } else {
            format!("unknown PortOpenError {}", self.0)
        };
        write!(f, "{}", message)
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("received a malformed message")]
    MalformedMessage,
    #[error("received an invalid message type")]
    InvalidMessageType,
    #[error("client got disconnected")]
    Disconnected,
    #[error("encoding message: {0}")]
    Encode(#[from] prost::EncodeError),
    #[error("decoding message: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("websocket error {0}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("error in library {0}")]
    Lib(#[from] libnp::Error),
    #[error("http client error {0}")]
    Hyper(#[from] hyper::Error),
    #[error("http client error {0}")]
    HyperHttp(#[from] hyper::http::Error),

    #[error("could not forward HTTP hostname {0}: {1}")]
    HttpOpenError(String, HttpOpenErrorMessage),
    #[error("HTTP hostname {0} is already registered")]
    HttpHostnameAlreadyRegistered(String),
    #[error("HTTP hostname {0} not registered")]
    HttpHostnameNotRegistered(String),

    #[error("Could not forward port {0}: {1}")]
    PortOpenError(Addr, PortOpenErrorMessage),
    #[error("Port origin {0} already registered")]
    PortOriginAlreadyRegistered(String),
    #[error("Port origin {0} not registered")]
    PortOriginNotRegistered(String),
    #[error("Port ID {0} already registered")]
    PortIDAlreadyRegistered(String),
    #[error("Port ID {0} not registered")]
    PortIDNotRegistered(String),
    #[error("Could not allocate remote port {0}: {1}")]
    RemotePortNotAllocated(String, String),
}

pub type Result<T> = std::result::Result<T, Error>;
