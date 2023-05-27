use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortReceive {
    pub uuid: String,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortReceived {}

pub enum PortMessage {
    Data(Vec<u8>),
    Close,
}
