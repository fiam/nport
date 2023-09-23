pub mod client;
pub mod common;
pub mod error;
pub mod server;

use std::fmt;
use std::net::{AddrParseError, SocketAddr};
use std::str::FromStr;

pub use error::Error;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use simple_error::SimpleError;

const DEFAULT_ADDR_HOST: &str = "127.0.0.1";
const LOCALHOST: &str = "localhost";

fn is_loopback(addr: &str) -> bool {
    std::net::IpAddr::from_str(addr)
        .map(|a| a.is_loopback())
        .ok()
        .unwrap_or_default()
}

/// An address with a hostname or IP and a port number
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Addr {
    host: Option<String>,
    port: u16,
}

impl Addr {
    pub fn from_port(port: u16) -> Self {
        Self { host: None, port }
    }

    pub fn from_host_and_port(host: &str, port: u16) -> Self {
        let host = if host.is_empty() || host == LOCALHOST || is_loopback(host) {
            None
        } else {
            Some(host.to_string())
        };
        Self { host, port }
    }

    pub fn has_host(&self) -> bool {
        self.host.is_some()
    }

    pub fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn with_port(&self, port: u16) -> Self {
        Self {
            host: self.host.clone(),
            port,
        }
    }

    pub fn try_to_socket_addr(&self) -> Result<SocketAddr, AddrParseError> {
        let addr: SocketAddr = self.to_string().parse()?;
        Ok(addr)
    }

    pub fn from_socket_addr(addr: &SocketAddr) -> Self {
        Addr::from_host_and_port(&addr.ip().to_string(), addr.port())
    }

    fn try_parse_socket_addr(s: &str) -> Result<Self, SimpleError> {
        let addr: SocketAddr = s.parse().map_err(SimpleError::from)?;
        Ok(Addr::from_socket_addr(&addr))
    }

    fn try_parse_hostname(s: &str) -> Result<Self, SimpleError> {
        let parts: Vec<_> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(SimpleError::new(format!("invalid host:port {}", s)));
        }
        let port: u16 = parts[1].parse().map_err(SimpleError::from)?;
        Ok(Addr::from_host_and_port(parts[0], port))
    }
}

impl FromStr for Addr {
    type Err = SimpleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Addr::try_parse_socket_addr(s).or_else(|_| Addr::try_parse_hostname(s))
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = self.host.as_deref().unwrap_or(DEFAULT_ADDR_HOST);
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        write!(f, "{}:{}", host, self.port)
    }
}

impl Serialize for Addr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.host {
            Some(_) => serializer.serialize_str(&self.to_string()),
            None => serializer.serialize_u16(self.port),
        }
    }
}

struct AddrVisitor;

impl<'de> Visitor<'de> for AddrVisitor {
    type Value = Addr;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a port number or an address in host:port form")
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(v as i64)
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v >= 0 && v <= i64::from(u16::MAX) {
            Ok(Addr {
                host: None,
                port: v as u16,
            })
        } else {
            Err(E::custom(format!(
                "port number must be >= 0 and < {}, not {}",
                u16::MAX,
                v
            )))
        }
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(v as i64)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let addr: Addr = v
            .parse()
            .map_err(|e| E::custom(format!("could not parse {} as an address: {}", v, e)))?;
        Ok(addr)
    }
}

impl<'de> Deserialize<'de> for Addr {
    fn deserialize<D>(deserializer: D) -> Result<Addr, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(AddrVisitor)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct WithAddr {
        pub addr: Addr,
    }

    #[test]
    fn test_serialize_addr_port() {
        let value = WithAddr {
            addr: Addr {
                host: None,
                port: 1234,
            },
        };
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(r#"{"addr":1234}"#, encoded);
    }

    #[test]
    fn test_serialize_addr_host_port() {
        let value = WithAddr {
            addr: Addr::from_host_and_port("example.com", 4567),
        };
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(r#"{"addr":"example.com:4567"}"#, encoded);
    }

    #[test]
    fn test_deserialize_addr_port() {
        let value = r#"{"addr": 1234}"#;
        let decoded: WithAddr = serde_json::from_str(value).unwrap();
        assert_eq!(1234, decoded.addr.port);
    }

    #[test]
    fn test_deserialize_addr_ipv4_port() {
        let value = r#"{"addr": "192.168.1.1:456"}"#;
        let decoded: WithAddr = serde_json::from_str(value).unwrap();
        assert_eq!(decoded.addr.host.as_deref(), Some("192.168.1.1"));
        assert_eq!(456, decoded.addr.port);
    }

    #[test]
    fn test_deserialize_addr_ipv6_port() {
        let value = r#"{"addr": "[2001:0:130F::9C0:876A:130B]:456"}"#;
        let decoded: WithAddr = serde_json::from_str(value).unwrap();
        assert_eq!(
            decoded.addr.host.as_deref(),
            Some("2001:0:130f::9c0:876a:130b")
        );
        assert_eq!(456, decoded.addr.port);
    }

    #[test]
    fn test_deserialize_addr_hostname_port() {
        let value = r#"{"addr": "example.com:3000"}"#;
        let decoded: WithAddr = serde_json::from_str(value).unwrap();
        assert_eq!(decoded.addr.host.as_deref(), Some("example.com"));
        assert_eq!(3000, decoded.addr.port);
    }

    #[test]
    fn test_deserialize_string_only_port() {
        let value = r#"{"addr": ":3000"}"#;
        let decoded: WithAddr = serde_json::from_str(value).unwrap();
        assert_eq!(decoded.addr.host, None);
        assert_eq!(3000, decoded.addr.port);
    }

    #[test]
    fn test_deserialize_localhost() {
        let aliases = [
            "localhost",
            "127.0.0.1",
            "[0000:0000:0000:0000:0000:0000:0000:0001]",
            "[::1]",
        ];
        for alias in aliases {
            let value = format!("{{\"addr\": \"{}:9000\"}}", alias);
            let decoded: WithAddr = serde_json::from_str(&value).unwrap();
            assert_eq!(decoded.addr.host, None);
            assert_eq!(9000, decoded.addr.port);
        }
    }
}
