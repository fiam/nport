use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::Result;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum OpenResult {
    Ok,
    AlreadyOpen,
    InUse,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Open {
    pub hostname: String,
    pub result: OpenResult,
}

impl Open {
    pub fn ok(hostname: &str) -> Self {
        Open {
            hostname: hostname.to_owned(),
            result: OpenResult::Ok,
        }
    }

    pub fn failed(result: OpenResult) -> Self {
        assert!(result != OpenResult::Ok);
        Open {
            hostname: "".to_owned(),
            result,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HTTPRequest {
    pub uuid: String,
    pub addr: String,
    pub protocol: String,
    pub method: String,
    pub uri: String,
    pub headers: HashMap<String, Vec<u8>>,
    pub body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Open(Open),
    HTTPRequest(HTTPRequest),
}

pub fn decode(data: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
