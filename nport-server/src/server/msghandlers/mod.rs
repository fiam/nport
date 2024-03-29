use std::{ops::ControlFlow, sync::Arc};

use anyhow::Result;
use libnp::messages::{self, client::payload::Message};

use super::{client::Client, state::SharedState};

mod common;
mod http;
mod port;

pub async fn msg(
    state: &SharedState,
    client: Arc<Client>,
    payload: messages::client::Payload,
) -> Result<ControlFlow<()>> {
    let Some(message) = payload.message else {
        return Ok(ControlFlow::Continue(()));
    };
    match message {
        /* HTTP */
        Message::HttpOpen(open) => http::open(state, client.clone(), open)
            .await
            .map(|_| ControlFlow::Continue(())),
        Message::HttpClose(close) => http::close(state, client.clone(), close)
            .await
            .map(|_| ControlFlow::Continue(())),
        Message::HttpResponse(response) => http::response(state, client.clone(), response)
            .await
            .map(|_| ControlFlow::Continue(())),
        /* Ports */
        Message::PortOpen(open) => port::open(state, client.clone(), open)
            .await
            .map(|_| ControlFlow::Continue(())),
        Message::PortConnected(connected) => port::connected(state, client.clone(), connected)
            .await
            .map(|_| ControlFlow::Continue(())),
        Message::PortReceive(received) => port::receive(state, client.clone(), received)
            .await
            .map(|_| ControlFlow::Continue(())),
        Message::PortClose(close) => port::close(state, client.clone(), close)
            .await
            .map(|_| ControlFlow::Continue(())),
    }
}
