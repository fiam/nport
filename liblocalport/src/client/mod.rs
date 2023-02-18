use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::Result;

#[derive(Serialize, Deserialize, Debug)]
pub struct Open {
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpResponse {
    pub uuid: String,
    pub headers: HashMap<String, Vec<u8>>,
    pub status_code: u16,
    pub body: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Open(Open),
    HttpResponse(HttpResponse),
}

pub fn decode(data: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
