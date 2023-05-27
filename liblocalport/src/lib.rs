pub fn add(left: usize, right: usize) -> usize {
    left + right
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
