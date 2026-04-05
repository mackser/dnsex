use clap::Parser;
use error::DnsexError;
use server::Server;

mod error;
mod handler;
mod server;

#[derive(Parser, Debug)]
#[command(author, version, about = "DnsEx - Created by coigner <coigner@tuta.com>", long_about = None)]
struct DnsexOptions {
    #[arg(short, long, default_value_t = 8053)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), DnsexError> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let opts = DnsexOptions::parse();
    let addr = "0.0.0.0";
    let server = Server::new(addr, opts.port);
    server.start().await?;

    Ok(())
}
