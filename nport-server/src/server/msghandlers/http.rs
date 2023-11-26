use std::sync::Arc;

use anyhow::Result;
use libnp::messages;
use libnp::messages::server::payload::Message;
use libnp::messages::server::{HttpCloseError, HttpClosed, HttpOpenError, HttpOpened};

use crate::server::{
    client::Client,
    msghandlers::common::{subdomain_for_forwarding, ValidationError},
    state::SharedState,
};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: messages::client::HttpOpen,
) -> Result<()> {
    let subdomain = match subdomain_for_forwarding(state, open.hostname.as_deref()) {
        Err(error) => {
            let error = match error {
                ValidationError::Invalid => HttpOpenError::Invalid,
                ValidationError::Disallowed => HttpOpenError::Disallowed,
            };
            return client
                .send(Message::HttpOpened(HttpOpened {
                    hostname: open.hostname.clone().unwrap_or_default(),
                    local_address: open.local_address,
                    error: Some(error.into()),
                }))
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
            .send(Message::HttpOpened(HttpOpened {
                hostname,
                local_address: open.local_address,
                error: Some(HttpOpenError::InUse.into()),
            }))
            .await;
    }
    tracing::debug!(hostname, "HTTP forwarding opened");
    state.stats().http_hostnames().inc();
    client
        .send(Message::HttpOpened(HttpOpened {
            hostname,
            local_address: open.local_address,
            error: None,
        }))
        .await
}

pub async fn close(
    state: &SharedState,
    client: Arc<Client>,
    close: messages::client::HttpClose,
) -> Result<()> {
    let hostname = &close.hostname;
    let error = if state
        .registry()
        .release_http_hostname(&client, hostname)
        .await
    {
        tracing::debug!(hostname, "HTTP forwarding closed");
        state.stats().http_hostnames().dec();
        None
    } else {
        Some(HttpCloseError::NotRegistered)
    };
    let response = Message::HttpClosed(HttpClosed {
        hostname: hostname.to_string(),
        error: error.map(|e| e.into()),
    });
    client.send(response).await
}

pub async fn response(
    _: &SharedState,
    client: Arc<Client>,
    response: messages::client::HttpResponse,
) -> Result<()> {
    client.send_http_response(response).await?;
    Ok(())
}
