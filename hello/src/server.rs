use hickory_server::ServerFuture;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::Mutex;

use crate::error::DnsexError;
use crate::handler::DnsHandler;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub domain: String,
    pub addr: String,
    pub port: u16,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct Server {
    pub config: ServerConfig,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn start(self) -> Result<(), DnsexError> {
        let addr: SocketAddr = format!("{}:{}", self.config.addr, self.config.port).parse()?;

        let handler = DnsHandler {
            server: Arc::new(self),
            transfers: Arc::new(Mutex::new(HashMap::new())),
        };

        let mut server = ServerFuture::new(handler);

        let udp_socket = UdpSocket::bind(&addr).await?;
        let tcp_listener = TcpListener::bind(&addr).await?;

        println!("DNS server started on {}", addr);

        server.register_socket(udp_socket);
        server.register_listener(tcp_listener, std::time::Duration::from_secs(30));
        server.block_until_done().await?;

        Ok(())
    }
}
