pub mod client;
pub mod common;
pub mod error;
pub mod server;

use std::{fmt, str::FromStr};

pub use error::Error;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct PortProtocolParseError(pub String);

impl fmt::Display for PortProtocolParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid port protocol {}", self.0)
    }
}

impl FromStr for PortProtocol {
    type Err = PortProtocolParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tcp" => Ok(Self::Tcp),
            _ => Err(PortProtocolParseError(s.to_owned())),
        }
    }
}
