use std::sync::Arc;

use anyhow::Result;
use libnp::{server::PortOpenError, Addr};

use crate::server::{client::Client, port_server, state::SharedState};

use super::common::normalized_client_subdomain;

pub fn validate_open(
    state: &SharedState,
    open: &libnp::client::PortOpen,
) -> Result<(), PortOpenError> {
    let remote_addr = &open.remote_addr;
    let remote_port = remote_addr.port();
    if remote_port > 0 && remote_port < 1024 {
        return Err(PortOpenError::PortNotAllowed);
    }
    if let Some(hostname) = remote_addr.host() {
        let normalized = normalized_client_subdomain(state, hostname);
        if normalized != state.hostnames().tcp_hostname() {
            return Err(PortOpenError::HostNotAllowed);
        }
    }
    Ok(())
}

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::PortOpen,
) -> Result<()> {
    use libnp::server;
    let hostname = state.hostnames().tcp_hostname();
    let validation = validate_open(state, &open);
    let result = if let Err(error) = validation {
        Err(error)
    } else {
        // TODO: Don't ignore the requested host
        let opened = port_server::server(client.clone(), hostname, &open.remote_addr).await;
        match opened {
            Ok(port) => {
                tracing::debug!(port=?port, "TCP forwarding opened");

                let port_num = port.port();
                client.register_port(port).await;

                Ok(port_num)
            }
            Err(error) => {
                tracing::warn!(remote_addr=?open.remote_addr, error=?error, "failed to open TCP port forwarding");
                Err(server::PortOpenError::InUse)
            }
        }
    };
    let port = result.ok().unwrap_or(open.remote_addr.port());
    let remote_addr = Addr::from_host_and_port(&hostname, port);

    client
        .send(&server::Message::PortOpened(server::PortOpened {
            protocol: open.protocol,
            remote_addr,
            local_addr: open.local_addr,
            error: result.err(),
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
