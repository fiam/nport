use std::sync::Arc;

use libnp::messages::server::payload::Message;

use crate::client::Client;
use crate::error::Result;

mod http;
mod port;

pub async fn message(client: Arc<Client>, msg: Message) -> Result<()> {
    match msg {
        Message::HttpOpened(opened) => {
            if let Err(err) = client.http_register(&opened).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(
                    hostname = opened.hostname,
                    local_addr = ?opened.local_address,
                    "HTTP host created"
                )
            }
        }
        Message::HttpRequest(req) => {
            tracing::debug!(request=?req, "incoming HTTP request");
            http::request(client.clone(), &req).await?
        }
        Message::HttpClosed(closed) => {
            if let Err(err) = client.http_deregister(&closed).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(hostname = closed.hostname, "HTTP host closed")
            }
        }
        Message::PortOpened(opened) => {
            if let Err(err) = client.port_register(&opened).await {
                tracing::error!(error=?err,?opened, "registering port forwarding");
            } else {
                tracing::info!(?opened, "port forwarding created")
            }
        }
        Message::PortConnect(connect) => {
            tracing::debug!(?connect, "port connect");
            if let Err(err) = port::connect(client.clone(), &connect).await {
                tracing::error!(error=?err,?connect, "connecting to port");
            }
        }
        Message::PortReceive(receive) => {
            tracing::debug!(
                uuid = receive.uuid,
                len = receive.data.len(),
                "received port data"
            );
            if let Err(err) = client.port_receive(&receive).await {
                tracing::error!(error=?err,uuid=receive.uuid, "writing to port");
            }
        }
        Message::PortClose(close) => {
            tracing::debug!(uuid = close.uuid, "requested to close port");
            if let Err(err) = client.port_close(&close).await {
                tracing::error!(error=?err,uuid=close.uuid, "closing port");
            }
        }
    }

    Ok(())
}
