use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("received an invalid message type")]
    InvalidMessageType,
    #[error("client got disconnected")]
    Disconnected,
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
    #[error("HTTP hostname {0} is already being used")]
    HttpHostnameAlreadyInUse(String),
    #[error("HTTP hostname {0} is invalid")]
    HttpHostnameInvalid(String),
    #[error("HTTP hostname {0} already registered")]
    HttpHostnameAlreadyRegistered(String),
    #[error("HTTP hostname {0} not registered")]
    HttpHostnameNotRegistered(String),
    #[error("Port origin {0} already registered")]
    PortOriginAlreadyRegistered(String),
    #[error("Port origin {0} not registered")]
    PortOriginNotRegistered(String),
    #[error("Port ID {0} already registered")]
    PortIDAlreadyRegistered(String),
    #[error("Port ID {0} not registered")]
    PortIDNotRegistered(String),
}

pub type Result<T> = std::result::Result<T, Error>;
