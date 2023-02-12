use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, RwLock};

use crate::handler::Handler;

#[derive(Default)]
pub struct Registry {
    map: RwLock<HashMap<String, Arc<RwLock<Handler>>>>,
}

impl Registry {
    pub async fn register(&self, hostname: &str, handler: Arc<RwLock<Handler>>) -> bool {
        let mut map = self.map.write().await;
        if map.contains_key(hostname) {
            false
        } else {
            map.insert(hostname.to_owned(), handler);
            true
        }
    }

    pub async fn get(&self, hostname: &str) -> Option<Arc<RwLock<Handler>>> {
        let map = self.map.read().await;
        map.get(hostname).cloned()
    }
}
