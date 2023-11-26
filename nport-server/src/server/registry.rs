use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use axum::extract::ws::WebSocket;
use tokio::sync::RwLock;

use super::client::Client;

#[derive(Default)]
pub struct Registry {
    claimed_http_hostnames: RwLock<HashSet<String>>,
    clients: RwLock<Vec<Arc<Client>>>,
}

impl Registry {
    pub async fn register(&self, socket: WebSocket, who: SocketAddr) -> Arc<Client> {
        tracing::debug!(who=?who, "register client");
        let client = Arc::new(Client::new(socket, who));
        self.clients.write().await.push(client.clone());
        client
    }

    pub async fn deregister(&self, client: Arc<Client>) -> bool {
        // TODO: decrement stats for HTTP hostnames
        tracing::debug!(who=?client.who(), "deregister client");
        let mut clients = self.clients.write().await;
        if let Some(index) = clients.iter().position(|item| Arc::ptr_eq(item, &client)) {
            clients.swap_remove(index);
            let mut claimed_http_hostnames = self.claimed_http_hostnames.write().await;
            client
                .http_hostnames()
                .await
                .iter()
                .for_each(|hostname| _ = claimed_http_hostnames.remove(hostname));
            true
        } else {
            false
        }
    }

    pub async fn claim_http_hostname(&self, client: &Client, hostname: &str) -> bool {
        if !self
            .claimed_http_hostnames
            .write()
            .await
            .insert(hostname.to_owned())
        {
            return false;
        }
        client.add_http_hostname(hostname).await;
        true
    }

    pub async fn release_http_hostname(&self, client: &Client, hostname: &str) -> bool {
        if !client.remove_http_hostname(hostname).await {
            return false;
        }
        self.claimed_http_hostnames.write().await.remove(hostname);
        true
    }

    pub async fn get_by_http_hostname(&self, hostname: &str) -> Option<Arc<Client>> {
        let mut client: Option<Arc<Client>> = None;
        for item in self.clients.read().await.iter() {
            if item.has_http_hostname(hostname).await {
                client = Some(item.clone());
                break;
            }
        }
        client
    }
}
