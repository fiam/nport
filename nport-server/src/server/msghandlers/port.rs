use std::sync::Arc;

use anyhow::Result;
use libnp::server::PortOpenResult;

use crate::server::{client::Client, port_server, state::SharedState};

pub fn validate_open(_: &SharedState, open: &libnp::client::PortOpen) -> PortOpenResult {
    let remote_port = open.remote_addr.port();
    if remote_port > 0 && remote_port < 1024 {
        return PortOpenResult::PortNotAllowed;
    }
    PortOpenResult::Ok
}

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::PortOpen,
) -> Result<()> {
    use libnp::server;
    let hostname = state.hostnames().tcp_hostname();
    let validation = validate_open(state, &open);
    let result = if validation != PortOpenResult::Ok {
        (validation, 0)
    } else {
        // TODO: Don't ignore the requested host
        let opened = port_server::server(client.clone(), hostname, &open.remote_addr).await;
        match opened {
            Ok(port) => {
                tracing::debug!(port=?port, "TCP forwarding opened");

                let port_num = port.port();
                client.register_port(port).await;

                (server::PortOpenResult::Ok, port_num)
            }
            Err(error) => {
                tracing::warn!(remote_addr=?open.remote_addr, error=?error, "failed to open TCP port forwarding");
                (server::PortOpenResult::PortInUse, 0)
            }
        }
    };

    client
        .send(&server::Message::PortOpened(server::PortOpened {
            protocol: open.protocol,
            hostname: hostname.to_string(),
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
