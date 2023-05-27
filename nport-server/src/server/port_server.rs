use std::{
    net::SocketAddr,
    ops::ControlFlow,
    sync::{Arc, Weak},
};

use anyhow::Result;
use libnp::{client::PortConnectedResult, common::PortMessage};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc,
        oneshot::{self, Sender},
    },
    task::yield_now,
    time::timeout,
};
use uuid::Uuid;

use super::client::Client;

#[derive(Debug)]
pub struct Port {
    closer: Option<Sender<()>>,
    port: u16,
}

impl Port {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        if let Some(closer) = self.closer.take() {
            _ = closer.send(());
        }
    }
}

async fn serve_socket(
    client: Weak<Client>,
    hostname: &str,
    port: u16,
    mut socket: TcpStream,
    from: SocketAddr,
) -> ControlFlow<()> {
    use libnp::server::{Message, PortClose, PortConnect, PortReceive};

    let uuid = Uuid::new_v4().to_string();

    let (messages_writer, mut messages_reader) = mpsc::channel::<libnp::common::PortMessage>(4);

    if let Some(client) = client.upgrade() {
        let rx = client.register_connect_request(&uuid).await;

        let connect = PortConnect {
            uuid: uuid.clone(),
            protocol: libnp::PortProtocol::Tcp,
            hostname: hostname.to_string(),
            port,
            from: from.to_string(),
        };

        if let Err(error) = client.send(&Message::PortConnect(connect)).await {
            tracing::warn!(error=?error, "sending PortConnect to client");
            return ControlFlow::Break(());
        }

        let response = match rx.await {
            Ok(resp) => resp,
            Err(error) => {
                tracing::warn!(error=?error, "receiving from PortConnect queue");
                return ControlFlow::Continue(());
            }
        };

        if let PortConnectedResult::Error(error) = response.result {
            tracing::debug!(error, "client couldn't connect");
            return ControlFlow::Continue(());
        }

        if let Err(error) = client.port_writer_register(&uuid, messages_writer).await {
            tracing::debug!(error=?error, uuid, "registering port writer");
            return ControlFlow::Continue(());
        }
    } else {
        return ControlFlow::Break(());
    }

    tokio::spawn(async move {
        tracing::trace!(uuid, "begin socket");
        let mut buf = vec![0; 512];
        let (mut port_reader, mut port_writer) = socket.split();
        let time_step = tokio::time::Duration::from_millis(50);
        'port: loop {
            tokio::select! {
                msg = messages_reader.recv() => {
                    if let Some(msg) = msg {
                        match msg {
                            PortMessage::Data(data) => {
                                tracing::trace!(uuid, length=data.len(), "write to port");
                                if let Err(error) = port_writer.write_all(&data).await {
                                    tracing::debug!(uuid, error=?error, "writing to port, closing");
                                    break 'port;
                                }
                            },
                            PortMessage::Close => {
                                tracing::trace!(uuid, "closing connection as requested");
                                break 'port;
                            }
                        }
                    }
                },
                read =  timeout(time_step, port_reader.read(&mut buf)) => {
                    // If we get the error case here, it means the timeout elapsed without receiving any data
                    if let Ok(result) = read {
                        match result {
                            Ok(size) => {
                                if let Some(client) = client.upgrade() {
                                    let msg = if size == 0 {
                                        tracing::trace!(uuid, "connection closed by client");
                                        let close = PortClose {
                                            uuid: uuid.clone()
                                        };
                                        Message::PortClose(close)
                                    } else {
                                        tracing::trace!(uuid, size, "sending data to client");
                                        let receive = PortReceive {
                                            uuid: uuid.clone(),
                                            data: buf[..size].to_vec(),
                                        };
                                        Message::PortReceive(receive)
                                    };
                                    if let Err(error) = client.send(&msg).await {
                                        tracing::warn!(error=?error, "error sending data to client, closing");
                                    }
                                    if size == 0 {
                                        break 'port;
                                    }
                                } else {
                                    // Client has been released
                                    tracing::trace!(uuid, "client was released");
                                    break 'port;
                                }
                            }
                            Err(error) => {
                                tracing::warn!(error=?error, "reading from forwarded socket");
                                break 'port;
                            }
                        }
                    } else {
                        // No new data, check if the weak reference to client is still alive
                        if client.upgrade().is_none() {
                            break 'port;
                        }
                        yield_now().await;
                    }
                }
            }
        }
        if let Some(client) = client.upgrade() {
            if let Err(error) = client.port_writer_remove(&uuid).await {
                tracing::debug!(error=?error, uuid, "removing port writer");
            }
        }
        tracing::trace!(uuid, "ending socket");
    });

    ControlFlow::Continue(())
}

pub async fn server(client: Arc<Client>, hostname: &str, port: u16) -> Result<Port> {
    let client = Arc::downgrade(&client);
    let hostname = hostname.to_string();
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    let addr = listener.local_addr()?;
    tracing::debug!(addr=?addr, "listening on port");
    let port = addr.port();
    let (closer_tx, mut closer_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        loop {
            let listener_future = listener.accept();
            tokio::select! {
                listener_result = listener_future => {
                    match listener_result {
                        Ok((socket, from)) => {
                            tracing::debug!(port, hostname, from=?from, "new port connection");
                            if let ControlFlow::Break(_) = serve_socket(client.clone(), &hostname, port, socket, from).await {
                                break;
                            }
                        },
                        Err(error) => {
                            tracing::warn!(error=?error, "accept port");
                            continue;
                        }
                    }
                },
                _ = &mut closer_rx => {
                    tracing::debug!(port, hostname, "port forwarding closed");
                    return;
                }
            };
        }
    });
    Ok(Port {
        closer: Some(closer_tx),
        port,
    })
}
