use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json;

use crate::error::Result;

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpOpen {
    pub hostname: String,
    pub local_port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpClose {
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpResponseError {
    NotRegistered,
    InvalidMethod(String),
    Build(String),
    Request(String),
    Read(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpResponseData {
    pub headers: HashMap<String, Vec<u8>>,
    pub status_code: u16,
    pub body: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum HttpResponsePayload {
    Error(HttpResponseError),
    Data(HttpResponseData),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpResponse {
    pub uuid: String,
    pub payload: HttpResponsePayload,
}

impl HttpResponse {
    pub fn error(uuid: &str, error: HttpResponseError) -> Self {
        Self {
            uuid: uuid.to_string(),
            payload: HttpResponsePayload::Error(error),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TcpOpen {
    pub hostname: String,
    pub port: u16,
    pub local_port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    HttpOpen(HttpOpen),
    HttpClose(HttpClose),
    HttpResponse(HttpResponse),
    TcpOpen(TcpOpen),
}

pub fn decode(data: &[u8]) -> Result<Message> {
    Ok(serde_json::from_slice(data)?)
}

pub fn encode(msg: &Message) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(msg)?)
}
