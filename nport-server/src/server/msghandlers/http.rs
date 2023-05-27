use std::sync::Arc;

use anyhow::Result;

use crate::server::{client::Client, state::SharedState};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::HttpOpen,
) -> Result<()> {
    use libnp::server;

    let hostname = if !open.hostname.is_empty() {
        open.hostname
    } else {
        names::Generator::with_naming(names::Name::Numbered)
            .next()
            .unwrap()
    };
    if !state
        .registry()
        .claim_http_hostname(&client, &hostname)
        .await
    {
        return client
            .send(&server::Message::HttpOpened(server::HttpOpened::failed(
                server::HttpOpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!(hostname, "HTTP forwarding opened");
    client
        .send(&server::Message::HttpOpened(server::HttpOpened::ok(
            &hostname,
            open.local_port,
        )))
        .await
}

pub async fn close(
    state: &SharedState,
    client: Arc<Client>,
    close: libnp::client::HttpClose,
) -> Result<()> {
    let hostname = &close.hostname;
    let result = if !state
        .registry()
        .release_http_hostname(&client, hostname)
        .await
    {
        libnp::server::HttpCloseResult::NotRegistered
    } else {
        tracing::debug!(hostname, "HTTP forwarding closed");
        libnp::server::HttpCloseResult::Ok
    };
    let response = libnp::server::Message::HttpClosed(libnp::server::HttpClosed {
        hostname: hostname.to_owned(),
        result,
    });
    client.send(&response).await
}

pub async fn response(
    _: &SharedState,
    client: Arc<Client>,
    response: libnp::client::HttpResponse,
) -> Result<()> {
    client.send_http_response(response).await?;
    Ok(())
}
