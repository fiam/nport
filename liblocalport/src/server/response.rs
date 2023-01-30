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

#[derive(Serialize, Deserialize)]
pub enum Response {
    Open(Open),
}

pub fn decode(data: &[u8]) -> Result<Response> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Response) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
