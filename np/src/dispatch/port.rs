use std::sync::Arc;

use crate::client::Client;
use crate::error::Result;

pub async fn connect(client: Arc<Client>, connect: &libnp::server::PortConnect) -> Result<()> {
    use libnp::client::{Message, PortConnected, PortConnectedResult};

    let client_copy = client.clone();
    let result = match client
        .port_connect(connect, |uuid, addr| async move {
            crate::client::port::start(client_copy, &uuid, addr).await
        })
        .await
    {
        Ok(()) => PortConnectedResult::Ok,
        Err(error) => PortConnectedResult::Error(error.to_string()),
    };
    client
        .send(&Message::PortConnected(PortConnected {
            uuid: connect.uuid.clone(),
            result,
        }))
        .await
}
