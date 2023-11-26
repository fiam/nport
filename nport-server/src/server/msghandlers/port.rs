use std::sync::Arc;

use anyhow::Result;
use libnp::{
    messages::{
        self,
        server::{PortOpenError, PortOpened},
    },
    Addr,
};

use libnp::messages::server::payload::Message;

use crate::server::{client::Client, port_server, state::SharedState};

use super::common::normalized_client_subdomain;

pub fn validate_open(
    state: &SharedState,
    open: &messages::client::PortOpen,
) -> Result<(), PortOpenError> {
    if let Some(remote_addr) = &open.remote_address {
        if remote_addr.port > 0 && remote_addr.port < 1024 {
            return Err(PortOpenError::PortNotAllowed);
        }
        if let Some(hostname) = &remote_addr.host {
            let normalized = normalized_client_subdomain(state, hostname);
            if normalized != state.hostnames().tcp_hostname() {
                return Err(PortOpenError::HostNotAllowed);
            }
        }
    }
    Ok(())
}

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: messages::client::PortOpen,
) -> Result<()> {
    use libnp::messages::server;
    let hostname = state.hostnames().tcp_hostname();
    let validation = validate_open(state, &open);
    let result = if let Err(error) = validation {
        Err(error)
    } else {
        let remote_address = Addr::from_address(open.remote_address.as_ref());
        // TODO: Don't ignore the requested host
        let opened = port_server::server(state, client.clone(), hostname, &remote_address).await;
        match opened {
            Ok(port) => {
                tracing::debug!(port=?port, "TCP forwarding opened");

                let port_num = port.port();
                client.register_port(port).await;

                Ok(port_num)
            }
            Err(error) => {
                tracing::warn!(remote_addr=?open.remote_address, error=?error, "failed to open TCP port forwarding");
                Err(server::PortOpenError::InUse)
            }
        }
    };
    let port = result
        .ok()
        .unwrap_or(Addr::from_address(open.remote_address.as_ref()).port());
    let remote_addr = Addr::from_host_and_port(hostname, port);

    client
        .send(Message::PortOpened(PortOpened {
            protocol: open.protocol,
            remote_address: Some(remote_addr.to_address()),
            local_address: open.local_address,
            error: result.err().map(|e| e.into()),
        }))
        .await
}

pub async fn connected(
    _state: &SharedState,
    client: Arc<Client>,
    connected: messages::client::PortConnected,
) -> Result<()> {
    client.send_connected_response(connected).await?;
    Ok(())
}

pub async fn receive(
    _state: &SharedState,
    client: Arc<Client>,
    receive: messages::client::PortReceive,
) -> Result<()> {
    client.port_receive(&receive).await?;
    Ok(())
}

pub async fn close(
    _state: &SharedState,
    client: Arc<Client>,
    close: messages::client::PortClose,
) -> Result<()> {
    client.port_close(&close).await?;
    Ok(())
}
