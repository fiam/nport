pub mod client;
pub mod common;
pub mod error;
pub mod server;

use std::fmt;

pub use error::Error;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum PortProtocol {
    Tcp,
    //    Udp,
}

impl fmt::Display for PortProtocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PortProtocol::Tcp => write!(f, "tcp"),
            //          PortProtocol::Udp => write!(f, "udp"),
        }
    }
}
