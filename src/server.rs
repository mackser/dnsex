use hickory_server::ServerFuture;
use std::net::SocketAddr;
use tokio::net::{TcpListener, UdpSocket};

use crate::error::DnsexError;
use crate::handler::DnsHandler;

#[derive(Debug, Clone)]
pub struct Server {
    pub domain: String,
    pub addr: String,
    pub port: u16,
}

impl Server {
    pub fn new<T>(domain: T, addr: T, port: u16) -> Self
    where
        T: Into<String>,
    {
        Self {
            domain: domain.into(),
            addr: addr.into(),
            port,
        }
    }

    pub async fn start(&self) -> Result<(), DnsexError> {
        let handler = DnsHandler {
            server: self.clone(),
        };

        let mut server = ServerFuture::new(handler);

        let addr: SocketAddr = format!("{}:{}", self.addr, self.port).parse()?;
        let udp_socket = UdpSocket::bind(&addr).await?;
        let tcp_listener = TcpListener::bind(&addr).await?;

        println!("DNS server started on {}", addr);

        server.register_socket(udp_socket);
        server.register_listener(tcp_listener, std::time::Duration::from_secs(30));
        server.block_until_done().await?;

        Ok(())
    }
}
