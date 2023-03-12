use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::Result;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpOpenResult {
    Ok,
    AlreadyOpen,
    InUse,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpOpen {
    pub hostname: String,
    pub local_port: u16,
    pub result: HttpOpenResult,
}

impl HttpOpen {
    pub fn ok(hostname: &str, local_port: u16) -> Self {
        Self {
            hostname: hostname.to_owned(),
            local_port,
            result: HttpOpenResult::Ok,
        }
    }

    pub fn failed(result: HttpOpenResult) -> Self {
        assert!(result != HttpOpenResult::Ok);
        Self {
            hostname: "".to_owned(),
            local_port: 0,
            result,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpCloseResult {
    Ok,
    NotRegistered,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpClose {
    pub hostname: String,
    pub result: HttpCloseResult,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpRequest {
    pub uuid: String,
    pub hostname: String,
    pub addr: String,
    pub protocol: String,
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, Vec<u8>>,
    pub body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    HttpOpen(HttpOpen),
    HttpClose(HttpClose),
    HttpRequest(HttpRequest),
}

pub fn decode(data: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
