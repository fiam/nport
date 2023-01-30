use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::Result;

#[derive(Serialize, Deserialize, Debug)]
pub struct Open {
    pub hostname: String,
}

#[derive(Serialize, Deserialize)]
pub enum Request {
    Open(Open),
}

pub fn decode(data: &[u8]) -> Result<Request> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Request) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}

pub fn open(hostname: &str) -> Result<Vec<u8>> {
    return encode(&Request::Open(Open {
        hostname: hostname.to_owned(),
    }));
}
