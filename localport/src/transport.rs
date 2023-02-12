use async_trait::async_trait;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing;

use liblocalport as lib;

use crate::error::{Error, Result};

#[async_trait]
pub trait Sender: Send {
    async fn send(&mut self, msg: &lib::client::Message) -> Result<()>;
}

#[async_trait]
pub trait Receiver: Send {
    async fn recv(&mut self) -> Result<lib::server::Message>;
}

#[async_trait]
pub trait Transport: Send {
    async fn connect(&self) -> Result<(Box<dyn Sender>, Box<dyn Receiver>)>;
    // async fn sender(&self) -> Result<&dyn Sender>;
    // async fn receiver(&self) -> Result<&dyn Receiver>;
}

// #[async_trait]
// pub trait Connector {
//     async fn connect(self) -> Result<Box<dyn Transport>>;
// }

pub fn ws(server: &str) -> impl Transport {
    WebSocketTransport::new(server)
}

// struct WebSocketConnector {
//     server: String,
// }

// impl WebSocketConnector {
//     pub fn new(server: &str) -> Self {
//         Self {
//             server: String::from(server),
//         }
//     }
// }

// #[async_trait]
// impl Connector for WebSocketConnector {
//     async fn connect(self) -> Result<Box<dyn Transport>> {
//         let (stream, response) = connect_async(&self.server).await?;
//         tracing::debug!("server response {:?}", response);
//         Ok(Box::new(WebSocketTransport::new(&self.server, stream)))
//     }
// }

struct WebSocketTransport {
    server: String,
}

impl WebSocketTransport {
    fn new(server: impl AsRef<str>) -> Self {
        Self {
            server: String::from(server.as_ref()),
        }
    }
}

struct WebSocketSender {
    sender_stream: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

#[async_trait]
impl Sender for WebSocketSender {
    async fn send(&mut self, msg: &lib::client::Message) -> Result<()> {
        let encoded = lib::client::encode(msg)?;
        return Ok(self.sender_stream.send(Message::Binary(encoded)).await?);
    }
}

struct WebSocketReceiver {
    receiver_stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

#[async_trait]
impl Receiver for WebSocketReceiver {
    async fn recv(&mut self) -> Result<lib::server::Message> {
        let received = self
            .receiver_stream
            .next()
            .await
            .ok_or(Error::Disconnected)??;
        match received {
            Message::Binary(data) => Ok(lib::server::decode(&data)?),
            _ => Err(Error::InvalidMessageType),
        }
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    // async fn sender(&self) -> Result<&dyn Sender> {
    //     return Ok(&self.sender);
    // }

    // async fn receiver(&self) -> Result<&dyn Receiver> {
    //     return Ok(&self.receiver);
    // }
    async fn connect(&self) -> Result<(Box<dyn Sender>, Box<dyn Receiver>)> {
        let (stream, response) = connect_async(&self.server).await?;
        tracing::debug!("server response {:?}", response);
        let (sender_stream, receiver_stream) = stream.split();
        let sender = Box::new(WebSocketSender { sender_stream });
        let receiver = Box::new(WebSocketReceiver { receiver_stream });
        Ok((sender, receiver))
    }
}
