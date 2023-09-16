pub mod client;
pub mod dispatch;
pub mod error;
mod settings;

use std::sync::Arc;

use clap::{ArgAction, Parser};
use libnp::Addr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use client::Client;

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
            Command::Http { local_port } => {
                tunnels.push(settings::Tunnel::Http(settings::HttpTunnel {
                    hostname: args.hostname,
                    local_addr: Addr::from_port(local_port),
                }))
            }
            Command::Tcp { local_port } => {
                tunnels.push(settings::Tunnel::Tcp(settings::TcpTunnel {
                    hostname: args.hostname,
                    remote_port: args.remote_port,
                    local_addr: Addr::from_port(local_port),
                }))
            }
        }
    }

    if tunnels.is_empty() {
        tracing::error!("no tunnels to run");
        return;
    }

    let client = Arc::new(Client::new());

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
            return;
        }
    }

    for tunnel in tunnels {
        let result = match tunnel {
            settings::Tunnel::Http(http) => {
                client
                    .http_open(&http.hostname.unwrap_or_default(), &http.local_addr)
                    .await
            }
            settings::Tunnel::Tcp(tcp) => {
                client
                    .tcp_open(
                        &tcp.hostname.unwrap_or_default(),
                        tcp.remote_port.unwrap_or_default(),
                        &tcp.local_addr,
                    )
                    .await
            }
        };
        if let Err(error) = result {
            tracing::error!(error=?error, "can't open connection");
            return;
        }
    }

    loop {
        match client.recv().await {
            Ok(msg) => {
                tracing::trace!(msg=?msg, "server message");
                if let Err(error) = dispatch::message(client.clone(), msg).await {
                    tracing::error!(error=?error, "handling server message");
                }
            }
            Err(error) => {
                tracing::error!(error=?error, "error receiving data from server");
                return;
            }
        }
    }
}
