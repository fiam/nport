use std::sync::Arc;

use libnp::messages::client::payload::Message;
use libnp::messages::client::{PortConnected, PortConnectedError};
use libnp::messages::server::PortConnect;

use crate::client::Client;
use crate::error::Result;

pub async fn connect(client: Arc<Client>, connect: &PortConnect) -> Result<()> {
    let client_copy = client.clone();
    let error = client
        .port_connect(connect, |uuid, addr| async move {
            crate::client::port::start(client_copy, &uuid, addr).await
        })
        .await
        .map_err(|e| PortConnectedError {
            error: e.to_string(),
        })
        .err();
    client
        .send(Message::PortConnected(PortConnected {
            uuid: connect.uuid.clone(),
            error,
        }))
        .await
}
