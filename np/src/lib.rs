pub mod client;
pub mod dispatch;
pub mod error;
mod settings;

use std::{process, sync::Arc, time::Duration};

use clap::{ArgAction, Parser};
use libnp::Addr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use client::{Client, VersionInfo};

use shadow_rs::shadow;

shadow!(build);

#[derive(clap::Parser)]
struct Arguments {
    #[arg(long, short = 'H')]
    hostname: Option<String>,
    #[arg(long, short = 'R')]
    remote_port: Option<u16>,
    #[arg(long, short = 'N', action=ArgAction::SetTrue)]
    no_config_file: Option<bool>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    Http { local_port: u16 },
    Tcp { local_port: u16 },
    Version,
}

static MAX_CONNECT_RETRIES: u32 = 5;
static CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);

pub async fn connect_failure(client: &Client, retries: &mut u32) {
    if let Err(error) = client.disconnect().await {
        tracing::error!(?error, "internal error reconnecting to server, exiting");
        process::exit(1);
    }
    *retries += 1;
    if *retries > MAX_CONNECT_RETRIES {
        tracing::error!("can't connect to server, giving up");
        process::exit(1);
    }
    // Increase delay with exponential backoff
    let delay = CONNECT_RETRY_DELAY * 2u32.pow(*retries);
    tracing::error!("can't connect to server, retrying in {:?}", delay);
    tokio::time::sleep(delay).await;
}

pub async fn run() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Arguments::parse();

    let use_config_file = !args.no_config_file.unwrap_or_default();
    let s = match settings::Settings::new(use_config_file) {
        Ok(settings) => settings,
        Err(error) => {
            tracing::error!(error=?error, "parsing configuration");
            return;
        }
    };

    let mut tunnels = s.tunnels.unwrap_or(Vec::new());
    if let Some(command) = args.command {
        match command {
            Command::Version => {
                let commit_suffix = if build::GIT_CLEAN { "" } else { "-dirty" };
                println!(
                    "nport {} ({}{}) built on {} with {} {}\n",
                    build::PKG_VERSION,
                    build::SHORT_COMMIT,
                    commit_suffix,
                    build::BUILD_TIME,
                    build::RUST_VERSION,
                    build::RUST_CHANNEL
                );
                println!("See nport.io for more information");
                process::exit(0);
            }
            Command::Http { local_port } => {
                tunnels.push(settings::Tunnel::Http(settings::HttpTunnel {
                    hostname: args.hostname,
                    local_addr: Addr::from_port(local_port),
                }))
            }
            Command::Tcp { local_port } => {
                let remote_addr = if let Some(hostname) = args.hostname {
                    Some(Addr::from_host_and_port(
                        &hostname,
                        args.remote_port.unwrap_or_default(),
                    ))
                } else {
                    args.remote_port.map(Addr::from_port)
                };

                tunnels.push(settings::Tunnel::Tcp(settings::TcpTunnel {
                    remote_addr,
                    local_addr: Addr::from_port(local_port),
                }))
            }
        }
    }

    if tunnels.is_empty() {
        tracing::error!("no tunnels to run");
        return;
    }

    let version_info = VersionInfo::new(build::PKG_VERSION, build::SHORT_COMMIT, !build::GIT_CLEAN);
    let client = Arc::new(Client::new(version_info));

    let mut connect_retries = 0;

    loop {
        match client.connect(&s.server.hostname, s.server.secure).await {
            Ok(()) => {
                tracing::info!(
                    server = s.server.hostname,
                    secure = s.server.secure,
                    "connected"
                );
            }
            Err(error) => {
                tracing::error!(error=?error, server=s.server.hostname, "can't connect to server");
                connect_failure(&client, &mut connect_retries).await;
                continue;
            }
        }

        for tunnel in &tunnels {
            let result = match tunnel {
                settings::Tunnel::Http(http) => {
                    let hostname = http.hostname.as_deref().unwrap_or("");
                    client.http_open(hostname, &http.local_addr).await
                }
                settings::Tunnel::Tcp(tcp) => {
                    let remote_addr = tcp.remote_addr.clone().unwrap_or_default();
                    client.tcp_open(&remote_addr, &tcp.local_addr).await
                }
            };
            if let Err(error) = result {
                tracing::error!(error=?error, "can't set up tunnels");
                connect_failure(&client, &mut connect_retries).await;
                continue;
            }
        }

        // Reset connect_retries back to zero
        connect_retries = 0;

        'read: loop {
            match client.recv().await {
                Ok(msg) => {
                    tracing::trace!(msg=?msg, "server message");
                    if let Err(error) = dispatch::message(client.clone(), msg).await {
                        tracing::error!(error=?error, "handling server message");
                    }
                }
                Err(error) => {
                    tracing::error!(error=?error, "receiving data from server, reconnecting");
                    break 'read;
                }
            }
        }
    }
}
