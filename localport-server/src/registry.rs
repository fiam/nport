use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;

use crate::handler::Handler;

#[derive(Default)]
pub struct Registry {
    map: Mutex<HashMap<String, Arc<Mutex<Handler>>>>,
}

impl Registry {
    pub async fn register(&self, hostname: &str, handler: Arc<Mutex<Handler>>) -> bool {
        let mut map = self.map.lock().await;
        if map.contains_key(hostname) {
            false
        } else {
            map.insert(hostname.to_owned(), handler);
            true
        }
    }

    pub async fn get(&self, hostname: &str) -> Option<Arc<Mutex<Handler>>> {
        let map = self.map.lock().await;
        map.get(hostname).cloned()
    }
}
