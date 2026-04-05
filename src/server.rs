use hickory_server::ServerFuture;
use std::net::SocketAddr;
use tokio::net::{TcpListener, UdpSocket};

use crate::error::DnsexError;
use crate::handler::DnsHandler;

pub struct Server {
    pub addr: String,
    pub port: u16,
}

impl Server {
    pub fn new(addr: impl Into<String>, port: u16) -> Self {
        Self {
            addr: addr.into(),
            port,
        }
    }

    pub async fn start(&self) -> Result<(), DnsexError> {
        let handler = DnsHandler;
        let mut server = ServerFuture::new(handler);

        let addr: SocketAddr = format!("{}:{}", self.addr, self.port).parse()?;
        let udp_socket = UdpSocket::bind(&addr).await?;
        let tcp_listener = TcpListener::bind(&addr).await?;

        tracing::info!("DNS server started on {}", addr);

        server.register_socket(udp_socket);
        server.register_listener(tcp_listener, std::time::Duration::from_secs(30));
        server.block_until_done().await?;

        Ok(())
    }
}
