use std::sync::Arc;

use crate::client::Client;
use crate::error::Result;

mod http;
mod port;

pub async fn message(client: Arc<Client>, msg: libnp::server::Message) -> Result<()> {
    use libnp::server;

    match msg {
        server::Message::HttpOpened(opened) => {
            if let Err(err) = client.http_register(&opened).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(
                    hostname = opened.hostname,
                    local_port = opened.local_port,
                    "HTTP host created"
                )
            }
        }
        server::Message::HttpRequest(req) => {
            tracing::debug!(request=?req, "incoming HTTP request");
            http::request(client.clone(), &req).await?
        }
        server::Message::HttpClosed(closed) => {
            if let Err(err) = client.http_deregister(&closed).await {
                tracing::error!(error=?err, "creating HTTP host");
            } else {
                tracing::info!(hostname = closed.hostname, "HTTP host closed")
            }
        }
        server::Message::PortOpened(opened) => {
            if let Err(err) = client.port_register(&opened).await {
                tracing::error!(error=?err,origin=opened.origin(), local_port=opened.local_port, "registering port forwarding");
            } else {
                tracing::info!(
                    origin = opened.origin(),
                    local_port = opened.local_port,
                    "port forwarding created"
                )
            }
        }
        server::Message::PortConnect(connect) => {
            tracing::debug!(origin = connect.origin(), "port connect");
            if let Err(err) = port::connect(client.clone(), &connect).await {
                tracing::error!(error=?err,origin=connect.origin(), "connecting to port");
            }
        }
        server::Message::PortReceive(receive) => {
            tracing::debug!(
                uuid = receive.uuid,
                len = receive.data.len(),
                "received port data"
            );
            if let Err(err) = client.port_receive(&receive).await {
                tracing::error!(error=?err,uuid=receive.uuid, "writing to port");
            }
        }
        server::Message::PortClose(close) => {
            tracing::debug!(uuid = close.uuid, "requested to close port");
            if let Err(err) = client.port_close(&close).await {
                tracing::error!(error=?err,uuid=close.uuid, "closing port");
            }
        }
    }

    Ok(())
}
