use std::sync::Arc;

use anyhow::Result;

use crate::server::{
    client::Client,
    msghandlers::common::{subdomain_for_forwarding, ValidationError},
    state::SharedState,
};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: libnp::client::HttpOpen,
) -> Result<()> {
    use libnp::server;

    let subdomain = match subdomain_for_forwarding(state, Some(&open.hostname)) {
        Err(error) => {
            let error = match error {
                ValidationError::Invalid => server::HttpOpenError::Invalid,
                ValidationError::Disallowed => server::HttpOpenError::Disallowed,
            };
            return client
                .send(&server::Message::HttpOpened(server::HttpOpened::error(
                    &open.hostname,
                    &open.local_addr,
                    error,
                )))
                .await;
        }
        Ok(hostname) => hostname.unwrap_or_else(|| {
            names::Generator::with_naming(names::Name::Numbered)
                .next()
                .unwrap()
        }),
    };
    let domain = state.hostnames().domain();
    let hostname = if domain.is_empty() {
        subdomain
    } else {
        format!("{}.{}", subdomain, domain)
    };
    if !state
        .registry()
        .claim_http_hostname(&client, &hostname)
        .await
    {
        return client
            .send(&server::Message::HttpOpened(server::HttpOpened::error(
                &hostname,
                &open.local_addr,
                server::HttpOpenError::InUse,
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
    let error = if state
        .registry()
        .release_http_hostname(&client, hostname)
        .await
    {
        tracing::debug!(hostname, "HTTP forwarding closed");
        None
    } else {
        Some(libnp::server::HttpCloseError::NotRegistered)
    };
    let response = libnp::server::Message::HttpClosed(libnp::server::HttpClosed {
        hostname: hostname.to_owned(),
        error,
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
