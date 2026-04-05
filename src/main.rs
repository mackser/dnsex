use hickory_server::ServerFuture;
use std::net::SocketAddr;
use tokio::net::{TcpListener, UdpSocket};
mod error;
use error::DnsexError;

mod handler;
use handler::DnsHandler;

#[tokio::main]
async fn main() -> Result<(), DnsexError>{
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let handler = DnsHandler;
    let mut server = ServerFuture::new(handler);

    let addr: SocketAddr = "0.0.0.0:8053".parse()?;

    let udp_socket = UdpSocket::bind(&addr).await?;
    let tcp_listener = TcpListener::bind(&addr).await?;

    tracing::info!("DNS server started on {}", addr);

    server.register_socket(udp_socket);
    server.register_listener(tcp_listener, std::time::Duration::from_secs(30));
    server.block_until_done().await?;

    Ok(())
}
