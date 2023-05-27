use std::sync::Arc;

use anyhow::Result;

use liblocalport as lib;

use crate::server::{client::Client, state::SharedState};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: lib::client::HttpOpen,
) -> Result<()> {
    use lib::server;

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
    close: lib::client::HttpClose,
) -> Result<()> {
    let hostname = &close.hostname;
    let result = if !state
        .registry()
        .release_http_hostname(&client, hostname)
        .await
    {
        lib::server::HttpCloseResult::NotRegistered
    } else {
        tracing::debug!(hostname, "HTTP forwarding closed");
        lib::server::HttpCloseResult::Ok
    };
    let response = lib::server::Message::HttpClosed(lib::server::HttpClosed {
        hostname: hostname.to_owned(),
        result,
    });
    client.send(&response).await
}

pub async fn response(
    _: &SharedState,
    client: Arc<Client>,
    response: lib::client::HttpResponse,
) -> Result<()> {
    client.send_http_response(response).await?;
    Ok(())
}
