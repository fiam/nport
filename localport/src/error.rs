use liblocalport;
use thiserror::Error;
use tokio_tungstenite;

#[derive(Error, Debug)]
pub enum Error {
    #[error("received an invalid message type")]
    InvalidMessageType,
    #[error("client got disconnected")]
    Disconnected,
    #[error("websocket error {0}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("error in library {0}")]
    Lib(#[from] liblocalport::Error),
}

pub type Result<T> = std::result::Result<T, Error>;