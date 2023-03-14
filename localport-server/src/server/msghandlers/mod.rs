use std::{ops::ControlFlow, sync::Arc};

use anyhow::Result;

use liblocalport as lib;

use super::{client::Client, state::SharedState};

mod http;
mod tcp;

pub async fn msg(
    state: &SharedState,
    client: Arc<Client>,
    msg: lib::client::Message,
) -> Result<ControlFlow<()>> {
    use lib::client::Message;

    match msg {
        /* HTTP */
        Message::HttpOpen(open) => {
            return http::open(state, client.clone(), open)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        Message::HttpClose(close) => {
            return http::close(state, client.clone(), close)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        Message::HttpResponse(response) => {
            return http::response(state, client.clone(), response)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        /* TCP */
        Message::TcpOpen(open) => {
            return tcp::open(state, client.clone(), open)
                .await
                .map(|_| ControlFlow::Continue(()))
        }
        _ => {
            tracing::error!(message = ?msg, "handled message type");
        }
    }
    Ok(ControlFlow::Continue(()))
}
