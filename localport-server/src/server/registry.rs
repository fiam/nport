use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use super::client::Client;

#[derive(Default)]
pub struct Registry {
    clients: RwLock<HashMap<String, Arc<RwLock<Client>>>>,
}

impl Registry {
    pub async fn register(&self, hostname: &str, client: Arc<RwLock<Client>>) -> bool {
        let mut clients = self.clients.write().await;
        if clients.contains_key(hostname) {
            false
        } else {
            clients.insert(hostname.to_owned(), client);
            true
        }
    }

    pub async fn deregister(&self, client: Arc<RwLock<Client>>) -> bool {
        let client = client.read().await;
        let hostname = client.hostname();
        let mut clients = self.clients.write().await;
        clients.remove(hostname).is_some()
    }

    pub async fn get(&self, hostname: &str) -> Option<Arc<RwLock<Client>>> {
        let clients = self.clients.read().await;
        clients.get(hostname).cloned()
    }
}
