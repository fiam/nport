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
    if !state.registry().claim_http_hostname(&hostname).await {
        return client
            .send(&server::Message::HttpOpen(server::HttpOpen::failed(
                server::HttpOpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!(hostname, "HTTP forwarding opened");
    client.add_http_hostname(&hostname).await;
    return client
        .send(&server::Message::HttpOpen(server::HttpOpen::ok(
            &hostname,
            open.local_port,
        )))
        .await;
}

pub async fn close(
    state: &SharedState,
    client: Arc<Client>,
    close: lib::client::HttpClose,
) -> Result<()> {
    let hostname = &close.hostname;
    let result = if !client.remove_http_hostname(hostname).await
        || !state.registry().release_http_hostname(hostname).await
    {
        lib::server::HttpCloseResult::NotRegistered
    } else {
        tracing::debug!(hostname, "HTTP forwarding closed");
        lib::server::HttpCloseResult::Ok
    };
    let response = lib::server::Message::HttpClose(lib::server::HttpClose {
        hostname: hostname.to_owned(),
        result,
    });
    return client.send(&response).await;
}

pub async fn response(
    _: &SharedState,
    client: Arc<Client>,
    response: lib::client::HttpResponse,
) -> Result<()> {
    client.send_response(response).await?;
    Ok(())
}
