use std::sync::Arc;

use anyhow::Result;

use crate::server::{client::Client, port_server, state::SharedState};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::PortOpen,
) -> Result<()> {
    use libnp::server;
    let result = match port_server::server(client.clone(), state.hostname(), &open.remote_addr)
        .await
    {
        Ok(port) => {
            tracing::debug!(port=?port, "TCP forwarding opened");

            let port_num = port.port();
            client.register_port(port).await;

            (server::PortOpenResult::Ok, port_num)
        }
        Err(error) => {
            tracing::warn!(remote_addr=?open.remote_addr, error=?error, "failed to open TCP port forwarding");
            (server::PortOpenResult::InUse, 0)
        }
    };
    client
        .send(&server::Message::PortOpened(server::PortOpened {
            protocol: open.protocol,
            hostname: state.hostname().to_string(),
            port: result.1,
            local_addr: open.local_addr,
            result: result.0,
        }))
        .await
}

pub async fn connected(
    _state: &SharedState,
    client: Arc<Client>,
    connected: libnp::client::PortConnected,
) -> Result<()> {
    client.send_connected_response(connected).await?;
    Ok(())
}

pub async fn receive(
    _state: &SharedState,
    client: Arc<Client>,
    receive: libnp::client::PortReceive,
) -> Result<()> {
    client.port_receive(&receive).await?;
    Ok(())
}

pub async fn close(
    _state: &SharedState,
    client: Arc<Client>,
    close: libnp::client::PortClose,
) -> Result<()> {
    client.port_close(&close).await?;
    Ok(())
}
