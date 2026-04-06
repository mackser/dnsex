use clap::{Parser, Subcommand};
use client::Client;
use error::DnsexError;
use server::Server;
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

        #[arg()]
        message: Option<String>,

        #[arg(short, long)]
        file: Option<String>,

        #[arg(short, long, default_value_t = 8053)]
        port: u16,
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
            message,
            file,
            port,
        } => {
            let payload: String = if let Some(msg) = message {
                msg
            } else if let Some(path) = file {
                fs::read_to_string(&path).await?
            } else {
                let mut buf = String::new();
                let mut stdin = io::stdin();
                stdin.read_to_string(&mut buf).await?;

                buf
            };

            let client = Client::new(domain, port);
            let _ = client.send_payload(payload.as_bytes()).await?;
        }
    };

    Ok(())
}
