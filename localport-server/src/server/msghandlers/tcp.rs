use std::sync::Arc;

use anyhow::Result;

use liblocalport as lib;

use crate::server::{client::Client, state::SharedState};

pub async fn open(
    state: &SharedState,
    client: Arc<Client>,
    open: lib::client::TcpOpen,
) -> Result<()> {
    use lib::server;

    Ok(())
}
