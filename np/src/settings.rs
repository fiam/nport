use config::{Config, ConfigError, Environment, File};
use libnp::Addr;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct HttpTunnel {
    pub hostname: Option<String>,
    pub local_addr: Addr,
}

#[derive(Debug, Deserialize)]
pub struct TcpTunnel {
    pub hostname: Option<String>,
    pub remote_addr: Option<Addr>,
    pub local_addr: Addr,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Tunnel {
    #[serde(rename = "http")]
    Http(HttpTunnel),
    #[serde(rename = "tcp")]
    Tcp(TcpTunnel),
}

#[derive(Debug, Deserialize)]
pub struct Server {
    pub hostname: String,
    pub secure: bool,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub server: Server,
    pub tunnels: Option<Vec<Tunnel>>,
}

static DEFAULT_SERVER: &str = "api.nport.io";

impl Settings {
    pub fn new(use_config_file: bool) -> Result<Self, ConfigError> {
        let mut builder = Config::builder()
            .set_default("server.hostname", DEFAULT_SERVER)
            .unwrap()
            .set_default("server.secure", true)
            .unwrap()
            .add_source(
                Environment::with_prefix("NP")
                    .try_parsing(true)
                    .separator("_"),
            );

        if use_config_file {
            builder = builder.add_source(File::with_name("nport").required(false));
        }

        let s = builder.build()?;
        s.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_ENV_MUTEX: std::sync::Mutex<i32> = std::sync::Mutex::new(1);

    #[test]
    fn no_config_works() {
        let _guard = TEST_ENV_MUTEX.lock().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(std::env::temp_dir()).unwrap();
        let s = Settings::new(true).unwrap();
        assert_eq!(s.server.hostname, DEFAULT_SERVER);
        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn override_server_from_env() {
        let _guard = TEST_ENV_MUTEX.lock().unwrap();
        let foobar = "foo.bar";
        let var = "NP_SERVER_HOSTNAME";
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(std::env::temp_dir()).unwrap();
        std::env::set_var(var, foobar);
        let s = Settings::new(true).unwrap();
        assert_eq!(s.server.hostname, foobar);
        std::env::set_current_dir(cwd).unwrap();
        std::env::remove_var(var);
    }
}
