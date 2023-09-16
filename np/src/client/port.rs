use std::sync::{Arc, Weak};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc::{self, Receiver},
    time::timeout,
};

use libnp::{common::PortMessage, Addr};

use crate::error::Result;

use super::Client;

fn client_port(
    client: Weak<Client>,
    mut stream: TcpStream,
    mut messages: Receiver<PortMessage>,
    uuid: &str,
) {
    let uuid = uuid.to_string();
    let time_step = tokio::time::Duration::from_millis(50);
    tokio::spawn(async move {
        let mut buf = vec![0; 512];
        let (mut port_reader, mut port_writer) = stream.split();
        'port: loop {
            tokio::select! {
                msg = messages.recv() => {
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
                                tracing::trace!(uuid, "close port");
                                break 'port;
                            }
                        }
                    }
                },
                read = timeout(time_step, port_reader.read(&mut buf)) => {
                    match read {
                        Ok(read_result) => {
                            match read_result {
                                Ok(size) => {
                                    if let Some(client) = client.upgrade() {
                                        let msg = if size == 0 {
                                            let close = libnp::client::PortClose {
                                                uuid: uuid.clone(),
                                            };
                                            libnp::client::Message::PortClose(close)
                                        } else {
                                            let receive = libnp::client::PortReceive {
                                                uuid: uuid.clone(),
                                                data: buf[..size].to_vec(),
                                            };
                                            libnp::client::Message::PortReceive(receive)
                                        };
                                        if let Err(error) = client.send(&msg).await {
                                            tracing::warn!(error=?error, "error sending data to client, closing");
                                            break 'port;
                                        }
                                        if size == 0 {
                                            // Socket was closed
                                            tracing::trace!(uuid, "connection closed");
                                            break 'port;
                                        }
                                    } else {
                                        tracing::debug!(uuid, "client released");
                                        break 'port;
                                    }
                                }
                                Err(error) => {
                                    tracing::debug!(uuid, error=?error, "reading from port");
                                }
                            }
                        }
                        Err(_elapsed) => {
                            // Timeout elapsed without reading any data
                            if client.strong_count() == 0 {
                                break 'port;
                            }
                        }
                    }
                },

            }
        }
        if let Some(client) = client.upgrade() {
            _ = client.port_writer_remove(&uuid).await;
        }
        tracing::debug!(uuid, "port forwarding closed");
    });
}

pub async fn start(client: Arc<Client>, uuid: &str, addr: Addr) -> Result<()> {
    tracing::trace!(uuid, ?addr, "connecting to port forwarding");
    let stream = TcpStream::connect(addr.to_string()).await?;
    let (message_sender, message_receiver) = mpsc::channel::<PortMessage>(1);
    client.port_writer_register(uuid, message_sender).await?;
    client_port(Arc::downgrade(&client), stream, message_receiver, uuid);
    Ok(())
}
