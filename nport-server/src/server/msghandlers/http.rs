use std::sync::Arc;

use anyhow::Result;

use crate::server::{client::Client, hostname, state::SharedState};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::HttpOpen,
) -> Result<()> {
    use libnp::server;

    let subdomain = if !open.hostname.is_empty() {
        // Don't allow invalid hostnames nor names with multiple labels
        if !hostname::is_valid(&open.hostname) || open.hostname.contains('.') {
            return client
                .send(&server::Message::HttpOpened(server::HttpOpened::failed(
                    &open.hostname,
                    &open.local_addr,
                    server::HttpOpenResult::Invalid,
                )))
                .await;
        }
        open.hostname
    } else {
        names::Generator::with_naming(names::Name::Numbered)
            .next()
            .unwrap()
    };
    let hostname = if state.domain().is_empty() {
        subdomain
    } else {
        format!("{}.{}", subdomain, state.domain())
    };
    if !state
        .registry()
        .claim_http_hostname(&client, &hostname)
        .await
    {
        return client
            .send(&server::Message::HttpOpened(server::HttpOpened::failed(
                &hostname,
                &open.local_addr,
                server::HttpOpenResult::InUse,
            )))
            .await;
    }
    tracing::debug!(hostname, "HTTP forwarding opened");
    client
        .send(&server::Message::HttpOpened(server::HttpOpened::ok(
            &hostname,
            &open.local_addr,
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
